use super::Heap;
use crate::trace::Tracer;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) struct StartStopJoinHandle {
    mutex: parking_lot::Mutex<(usize, Option<JoinHandle<()>>)>,
    condvar: parking_lot::Condvar,
}

impl StartStopJoinHandle {
    pub(super) fn new() -> Self {
        Self { mutex: parking_lot::Mutex::new((0, None)), condvar: parking_lot::Condvar::new() }
    }

    fn start(&self, f: impl FnOnce(StopCondition) + Send + 'static) -> bool {
        let mut guard = self.mutex.lock();
        if guard.1.is_some() {
            return false;
        }
        let counter = guard.0 + 1;
        guard.0 = counter;
        let c = StopCondition(counter);
        guard.1 = Some(std::thread::spawn(move || f(c)));
        true
    }

    fn stop(&self) -> bool {
        let handle = {
            let mut stopped = self.mutex.lock();
            if let Some(handle) = stopped.1.take() {
                self.condvar.notify_all();
                handle
            } else {
                return false;
            }
        };
        // A background collector panic is an invariant failure and is propagated to the stopper.
        handle.join().unwrap();
        true
    }

    fn wait_stopped(&self, c: StopCondition, timeout: Duration) -> bool {
        let mut stopped = self.mutex.lock();
        if stopped.1.is_none() || stopped.0 != c.0 {
            return true;
        }
        let result = self.condvar.wait_for(&mut stopped, timeout);
        !result.timed_out()
    }

    fn is_stopped(&self, c: StopCondition) -> bool {
        let stopped = self.mutex.lock();
        stopped.1.is_none() || stopped.0 != c.0
    }

    fn is_started(&self) -> bool {
        let stopped = self.mutex.lock();
        stopped.1.is_some()
    }
}

impl Drop for StartStopJoinHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Copy, Clone)]
struct StopCondition(usize);

impl Heap {
    pub fn start_background_collection(self: &Arc<Self>) -> bool {
        if self.options.is_background_collection_off() || self.bg_thread.is_started() {
            return false;
        }

        let heap_clone = Arc::clone(self);
        self.bg_thread.start(move |c| {
            background_gc_thread(heap_clone, c);
        })
    }

    pub fn stop_background_collection(&self) -> bool {
        self.bg_thread.stop()
    }
}

/// Background GC thread that performs incremental marking and sweeping.
fn background_gc_thread(heap: Arc<Heap>, c: StopCondition) {
    let tracer = Tracer::new();
    while !heap.options.collection_interval.is_zero()
        && !heap.bg_thread.wait_stopped(c, heap.options.collection_interval)
    {
        if heap.should_collect() && heap.try_start_marking() {
            heap.do_mark_roots(&tracer);

            loop {
                if heap.bg_thread.is_stopped(c) {
                    heap.finish_gc();
                    return;
                }

                let marking_complete = heap.do_mark_incremental(heap.options.incremental_work_budget);
                if marking_complete {
                    if !heap.yield_once_if_marking_busy() {
                        break;
                    }
                } else {
                    std::thread::yield_now();
                }
            }

            heap.sweep_and_finish();
        }
    }
}
