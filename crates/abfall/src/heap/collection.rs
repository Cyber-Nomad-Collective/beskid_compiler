use super::model::state_phase;
use super::{GcPhase, Heap};
use crate::gc_box::GcHeader;
use crate::trace::Tracer;
use std::sync::atomic::{AtomicPtr, Ordering};

impl Heap {
    fn update_threshold(&self, live_bytes: usize) {
        let old_threshold = self.current_threshold.load(Ordering::Relaxed);
        let new_threshold = self.options.calculate_threshold(old_threshold, live_bytes);
        self.current_threshold.store(new_threshold, Ordering::Relaxed);
    }

    pub fn should_collect(&self) -> bool {
        if self.options.is_completely_off() {
            return false;
        }

        let allocated = self.bytes_allocated.load(Ordering::Relaxed);
        let threshold = self.current_threshold.load(Ordering::Relaxed);

        if !self.options.is_limit_off() && allocated > self.options.limit_bytes {
            return true;
        }

        allocated > threshold
    }

    pub fn force_collect(&self) -> usize {
        loop {
            if self.try_mark_full() {
                return self.sweep_and_finish();
            }
            std::thread::yield_now();
        }
    }

    /// Test hook: start a marking cycle without sweeping.
    #[doc(hidden)]
    pub fn mark_for_tests(&self) -> bool {
        self.try_mark_full()
    }

    /// Test hook: sweep and finish an in-progress cycle.
    #[doc(hidden)]
    pub fn sweep_for_tests(&self) -> usize {
        self.sweep_and_finish()
    }

    pub fn collect(&self) {
        if self.should_collect() {
            self.force_collect();
        }
    }

    /// Check if GC is currently in marking phase.
    pub fn is_marking(&self) -> bool {
        self.gc_phase() == GcPhase::Marking
    }

    pub fn gc_phase(&self) -> GcPhase {
        state_phase(self.phase_state.load(Ordering::Acquire))
    }

    pub(crate) fn try_mark_full(&self) -> bool {
        if !self.try_start_marking() {
            return false;
        }

        self.wait_for_mutator_quiescence();

        {
            let tracer = Tracer::new();

            // STW pause: scan roots
            self.do_mark_roots(&tracer);

            // Concurrent marking
            self.do_mark_work_full(&tracer);
        }
        true
    }

    pub(crate) fn sweep_and_finish(&self) -> usize {
        self.start_sweeping();
        self.wait_for_mutator_quiescence();

        let tracer = Tracer::new();
        self.do_mark_work_full(&tracer);
        let live_bytes = self.do_sweep();
        self.update_threshold(live_bytes);
        self.finish_gc();
        live_bytes
    }

    pub(super) fn do_mark_roots(&self, tracer: &Tracer) {
        let mut current = self.head.load(Ordering::Acquire);
        while !current.is_null() {
            // SAFETY: allocations remain linked and live during marking; acquire observes the
            // release publication of both this header and its next link.
            unsafe {
                let header = &*current;
                if header.is_root() {
                    tracer.mark_header(header);
                }
                current = header.next.load(Ordering::Acquire);
            }
        }

        for root in self.external_roots.snapshot_roots() {
            self.mark_payload_ptr(root, tracer);
        }

        self.merge_work(tracer);
    }

    fn do_sweep(&self) -> usize {
        self.start_sweeping();

        let mut freed = 0;

        // SAFETY: sweeping starts only after the phase transition and mutator quiescence. The
        // collector has exclusive authority to unlink and drop white nodes in the intrusive list.
        unsafe {
            let mut current = self.head.load(Ordering::Acquire);
            let mut prev_next: *const AtomicPtr<GcHeader> = &self.head;

            while !current.is_null() {
                let header = &*current;
                let next = header.next.load(Ordering::Acquire);

                if header.is_white() {
                    self.beskid_allocations.unregister(current);
                    (*prev_next).store(next, Ordering::Release);

                    let size = header.vtable.layout.size();
                    (header.vtable.drop)(current);
                    freed += size;

                    current = next;
                } else {
                    header.reset_white();

                    prev_next = &header.next;
                    current = next;
                }
            }
        }

        self.bytes_allocated.fetch_sub(freed, Ordering::Relaxed) - freed
    }
}
