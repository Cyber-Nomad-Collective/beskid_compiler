use super::model::{gc_state, state_epoch, state_phase};
use super::{GcPhase, Heap};
use crate::gc_box::GcHeader;
use crate::trace::Tracer;
use std::sync::atomic::Ordering;

/// Shared gray work owned by the heap between local tracer passes.
pub(super) struct GrayQueue(pub(super) Vec<*const GcHeader>);

// SAFETY: pointers in the queue refer to GC headers whose lifetime is owned by `Heap`. Sweep only
// frees them after the collector transitions to sweeping and all mutator/marking work is quiescent.
unsafe impl Send for GrayQueue {}
// SAFETY: all access to the vector is serialized by `Heap::gray_queue`; sharing the wrapper does
// not permit concurrent access to the pointed-to headers outside the collector protocol.
unsafe impl Sync for GrayQueue {}

impl GrayQueue {
    pub(super) fn new() -> Self {
        Self(Vec::new())
    }
}

pub(super) struct MutatorOperationGuard<'heap> {
    heap: &'heap Heap,
    busy_marking: bool,
}

impl Drop for MutatorOperationGuard<'_> {
    fn drop(&mut self) {
        if self.busy_marking {
            self.heap.busy_marking_count.fetch_sub(1, Ordering::AcqRel);
        }
        self.heap.active_mutator_count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl MutatorOperationGuard<'_> {
    fn is_marking(&self) -> bool {
        self.busy_marking
    }
}

impl Heap {
    pub(super) fn enter_mutator_operation(&self) -> MutatorOperationGuard<'_> {
        loop {
            let before = self.phase_state.load(Ordering::SeqCst);
            if state_phase(before) == GcPhase::Sweeping {
                std::thread::yield_now();
                continue;
            }

            self.active_mutator_count.fetch_add(1, Ordering::SeqCst);
            if self.phase_state.load(Ordering::SeqCst) == before {
                let busy_marking = state_phase(before) == GcPhase::Marking;
                if busy_marking {
                    self.busy_marking_count.fetch_add(1, Ordering::AcqRel);
                }
                return MutatorOperationGuard { heap: self, busy_marking };
            }

            self.active_mutator_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(crate) fn with_mutator_operation<R>(&self, operation: impl FnOnce(bool) -> R) -> R {
        let guard = self.enter_mutator_operation();
        operation(guard.is_marking())
    }

    pub(super) fn wait_for_mutator_quiescence(&self) {
        while self.active_mutator_count.load(Ordering::SeqCst) != 0 {
            std::thread::yield_now();
        }
    }

    /// Try to transition to marking phase.
    pub(super) fn try_start_marking(&self) -> bool {
        loop {
            let state = self.phase_state.load(Ordering::Acquire);
            if state_phase(state) != GcPhase::Idle {
                return false;
            }
            let next = gc_state(GcPhase::Marking, state_epoch(state).wrapping_add(1));
            if self.phase_state.compare_exchange(state, next, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return true;
            }
        }
    }

    /// Transition to sweeping phase.
    pub(super) fn start_sweeping(&self) {
        loop {
            let state = self.phase_state.load(Ordering::SeqCst);
            match state_phase(state) {
                GcPhase::Sweeping => return,
                GcPhase::Marking => {
                    let next = gc_state(GcPhase::Sweeping, state_epoch(state));
                    if self.phase_state.compare_exchange(state, next, Ordering::SeqCst, Ordering::Acquire).is_ok() {
                        return;
                    }
                }
                GcPhase::Idle => panic!("cannot start sweeping while the collector is idle"),
            }
        }
    }

    /// Transition back to idle phase.
    pub(super) fn finish_gc(&self) {
        loop {
            let state = self.phase_state.load(Ordering::Acquire);
            if state_phase(state) == GcPhase::Idle {
                return;
            }
            let next = gc_state(GcPhase::Idle, state_epoch(state));
            if self.phase_state.compare_exchange(state, next, Ordering::Release, Ordering::Acquire).is_ok() {
                return;
            }
        }
    }

    /// Steal work from the shared gray queue into a tracer.
    ///
    /// Returns true if work was stolen, false if queue is empty.
    fn steal_work(&self, tracer: &Tracer, max_items: usize) -> bool {
        let mut gray_queue = self.gray_queue.lock();
        tracer.steal_from(max_items, &mut gray_queue.0)
    }

    /// Merge tracer's local work back to the shared gray queue.
    pub(crate) fn merge_work(&self, tracer: &Tracer) {
        let mut gray_queue = self.gray_queue.lock();
        tracer.append_to(&mut gray_queue.0);
    }

    /// Process marking work using a tracer.
    ///
    /// Steals work, processes it locally, then merges new work back.
    fn do_mark_with_tracer(&self, tracer: &Tracer, work_budget: usize) -> usize {
        let mut work_done = 0;

        while work_done < work_budget {
            let ptr = if let Some(p) = tracer.pop_work() {
                p
            } else {
                const BATCH_SIZE: usize = 8;
                if !self.steal_work(tracer, BATCH_SIZE) {
                    break;
                }
                continue;
            };

            // SAFETY: gray-queue entries are live GC headers. Sweep cannot run until marking work
            // and mutators are quiescent, and each vtable trace function accepts its own header.
            unsafe {
                let header = &*ptr;
                (header.vtable.trace)(ptr, tracer);
                header.mark_black();
            }

            work_done += 1;
        }

        if tracer.has_work() {
            self.merge_work(tracer);
        }

        work_done
    }

    /// Perform a bounded amount of incremental marking work.
    ///
    /// Returns true if marking is complete, false if more work remains.
    pub(super) fn do_mark_incremental(&self, work_budget: usize) -> bool {
        let tracer = Tracer::new();
        let work_done = self.do_mark_with_tracer(&tracer, work_budget);
        work_done == 0
    }

    pub(super) fn yield_once_if_marking_busy(&self) -> bool {
        if self.busy_marking_count.load(Ordering::Acquire) > 0 {
            std::thread::yield_now();
            true
        } else {
            false
        }
    }

    pub(super) fn do_mark_work_full(&self, tracer: &Tracer) {
        while self.do_mark_with_tracer(tracer, self.options.incremental_work_budget) > 0
            || self.yield_once_if_marking_busy()
        {}
    }
}
