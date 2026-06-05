//! Heap management and object storage
//!
//! This module provides the heap structure that stores GC-managed objects
//! and implements the mark and sweep phases of garbage collection.

use crate::beskid::{BeskidObject, TypeDescriptor};
use crate::gc_box::{GcBox, GcHeader};
use crate::ptr::GcRoot;
use crate::roots::ExternalRootSet;
use crate::trace::{Trace, Tracer};
use std::collections::HashMap;
use std::ptr::null_mut;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// Send-safe wrapper for raw pointer queue
struct GrayQueue(Vec<*const GcHeader>);

unsafe impl Send for GrayQueue {}
unsafe impl Sync for GrayQueue {}

impl GrayQueue {
    fn new() -> Self {
        Self(Vec::new())
    }
}

struct StartStopJoinHandle {
    mutex: parking_lot::Mutex<(usize, Option<JoinHandle<()>>)>,
    condvar: parking_lot::Condvar,
}

impl StartStopJoinHandle {
    fn new() -> Self {
        Self {
            mutex: parking_lot::Mutex::new((0, None)),
            condvar: parking_lot::Condvar::new(),
        }
    }

    fn start(&self, f: impl FnOnce(StopCondition) + Send + 'static) -> bool {
        let mut guard = self.mutex.lock();
        if guard.1.is_some() {
            return false; // already started
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
                return false; // already stopped
            }
        };
        handle.join().unwrap();
        true
    }

    fn wait_stopped(&self, c: StopCondition, timeout: Duration) -> bool {
        let mut stopped = self.mutex.lock();
        if stopped.1.is_none() || stopped.0 != c.0 {
            return true; // already stopped
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

/// GC phase states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GcPhase {
    /// GC is idle, no collection in progress
    Idle = 0,
    /// GC is marking reachable objects
    Marking = 1,
    /// GC is sweeping unreachable objects
    Sweeping = 2,
}

impl From<u8> for GcPhase {
    fn from(value: u8) -> Self {
        match value {
            1 => GcPhase::Marking,
            2 => GcPhase::Sweeping,
            _ => GcPhase::Idle,
        }
    }
}

impl From<GcPhase> for u8 {
    fn from(value: GcPhase) -> Self {
        value as u8
    }
}

/// Point-in-time heap statistics for runtime diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcHeapStats {
    pub bytes_allocated: usize,
    pub object_count: usize,
    pub phase: GcPhase,
    pub collection_threshold: usize,
    pub external_root_count: usize,
}

/// The garbage collected heap
///
/// Manages allocation and deallocation of GC objects using an intrusive
/// linked list, and implements the mark and sweep collection algorithm
/// with incremental marking support.
pub struct Heap {
    /// Head of the intrusive linked list of allocations
    head: AtomicPtr<GcHeader>,
    /// Garbage collection options
    options: GcOptions,
    /// Total bytes currently allocated
    bytes_allocated: AtomicUsize,
    /// Current collection threshold in bytes
    current_threshold: AtomicUsize,
    /// Gray queue for incremental marking
    gray_queue: parking_lot::Mutex<GrayQueue>,
    /// Current GC phase
    phase: AtomicU8,
    /// Background GC thread handle
    bg_thread: StartStopJoinHandle,
    /// Number of assist mutators or write-barriers active.
    busy_marking_count: std::sync::atomic::AtomicUsize,
    /// Runtime-registered roots and temporary handles.
    external_roots: ExternalRootSet,
    /// Mapping from payload pointer (`*mut u8`) to owning GC header.
    payload_to_header: parking_lot::Mutex<HashMap<usize, usize>>,
    /// Reverse mapping used to clean up payload map entries on sweep.
    header_to_payload: parking_lot::Mutex<HashMap<usize, usize>>,
}

#[derive(Clone, Copy, Debug)]
pub struct GcOptions {
    /// Interval between background collection attempts.
    ///
    /// If set to 0, background collection is disabled
    pub collection_interval: Duration,
    /// Work budget for incremental marking steps in background collection
    pub incremental_work_budget: usize,
    /// Work budget for mutator assist (0 = disabled)
    pub assist_work_budget: usize,
    /// Percentage threshold for triggering collection
    ///
    /// This is the percentage of additional memory usage since the last collection
    /// that will trigger a new collection.
    ///
    /// If set to 0 or usize::MAX, threshold-based collection is disabled
    pub threshold_percent: usize,
    /// Percentage threshold for shrinking the collection threshold
    ///
    /// Only applies the new threshold if the calculated threshold shrinks significantly.
    /// (below this percentage)
    ///
    /// 100 means always shrink, 0 means never shrink
    pub threshold_shrink_percent: usize,
    /// Initial & minimum threshold in bytes to trigger collection
    pub min_threshold_bytes: usize,
    /// Maximum allowed heap size in bytes
    pub limit_bytes: usize,
}

impl GcOptions {
    pub const DEFAULT: Self = Self {
        collection_interval: Duration::from_millis(100),
        incremental_work_budget: 100,
        assist_work_budget: 5,
        threshold_percent: 30,
        threshold_shrink_percent: 30,
        min_threshold_bytes: 1024 * 1024,
        limit_bytes: usize::MAX,
    };
    pub const OFF: Self = Self {
        collection_interval: Duration::from_millis(0),
        incremental_work_budget: usize::MAX,
        assist_work_budget: 0,
        threshold_percent: usize::MAX,
        threshold_shrink_percent: 0,
        min_threshold_bytes: usize::MAX,
        limit_bytes: usize::MAX,
    };
    /// Beskid runtime defaults: concurrent/background collection with heap-growth pacing.
    pub const BESKID_DEFAULT: Self = Self {
        collection_interval: Duration::from_millis(100),
        incremental_work_budget: 128,
        assist_work_budget: 8,
        threshold_percent: 100,
        threshold_shrink_percent: 30,
        min_threshold_bytes: 1024 * 1024,
        limit_bytes: usize::MAX,
    };
    /// Faster collection profile for deterministic tests.
    pub const BESKID_TEST: Self = Self {
        collection_interval: Duration::from_millis(10),
        incremental_work_budget: 512,
        assist_work_budget: 16,
        threshold_percent: 30,
        threshold_shrink_percent: 100,
        min_threshold_bytes: 64 * 1024,
        limit_bytes: usize::MAX,
    };

    #[inline]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    #[inline]
    pub const fn off() -> Self {
        Self::OFF
    }

    #[inline]
    pub const fn beskid_default() -> Self {
        Self::BESKID_DEFAULT
    }

    #[inline]
    pub const fn beskid_test() -> Self {
        Self::BESKID_TEST
    }

    #[inline]
    fn is_threshold_off(&self) -> bool {
        self.threshold_percent == 0 || self.threshold_percent == !0
    }

    #[inline]
    fn is_limit_off(&self) -> bool {
        self.limit_bytes == usize::MAX
    }

    #[inline]
    fn is_background_collection_off(&self) -> bool {
        self.is_threshold_off() || self.collection_interval.as_millis() == 0
    }

    #[inline]
    fn is_completely_off(&self) -> bool {
        self.is_threshold_off() && self.is_limit_off()
    }

    /// pacing
    fn calculate_threshold(&self, old_threshold: usize, live_usage: usize) -> usize {
        if self.is_threshold_off() {
            usize::MAX
        } else {
            let new_threshold = live_usage + (live_usage * self.threshold_percent) / 100;
            if new_threshold < old_threshold {
                if self.threshold_shrink_percent == 0 {
                    return old_threshold;
                } else if self.threshold_shrink_percent < 100 {
                    let shrink_limit = (old_threshold * self.threshold_shrink_percent) / 100;
                    if new_threshold > shrink_limit {
                        return old_threshold;
                    }
                }
            }
            if new_threshold < self.min_threshold_bytes {
                self.min_threshold_bytes
            } else {
                new_threshold
            }
        }
    }
}

impl Default for GcOptions {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Heap {
    pub fn new() -> Arc<Self> {
        Self::with_options(GcOptions::new())
    }

    pub fn off() -> Arc<Self> {
        Self::with_options(GcOptions::off())
    }

    pub fn with_options(options: GcOptions) -> Arc<Self> {
        let current_threshold = AtomicUsize::new(options.min_threshold_bytes);
        let heap = Arc::new(Self {
            head: AtomicPtr::new(null_mut()),
            options,
            bytes_allocated: AtomicUsize::new(0),
            current_threshold,
            gray_queue: parking_lot::Mutex::new(GrayQueue::new()),
            phase: AtomicU8::new(GcPhase::Idle as u8),
            bg_thread: StartStopJoinHandle::new(),
            busy_marking_count: std::sync::atomic::AtomicUsize::new(0),
            external_roots: ExternalRootSet::default(),
            payload_to_header: parking_lot::Mutex::new(HashMap::new()),
            header_to_payload: parking_lot::Mutex::new(HashMap::new()),
        });

        heap.start_background_collection();

        heap
    }

    pub fn allocate<T: Trace>(&self, data: T) -> GcRoot<T> {
        // Mutator assist: help with marking if enabled
        if self.options.assist_work_budget > 0 && self.check_is_marking_and_increment_busy() {
            self.do_mark_incremental(self.options.assist_work_budget);
            self.decrement_busy_marking();
        }

        let ptr = GcBox::new(data);
        let header_ptr = unsafe { &(*ptr.as_ptr()).header as *const GcHeader as *mut GcHeader };
        self.insert_allocation(header_ptr);

        // Return as GcRoot (already rooted with root_count = 1)
        unsafe { GcRoot::new_from_nonnull(ptr) }
    }

    /// Allocate an opaque Beskid payload tracked by descriptor metadata.
    pub fn allocate_beskid(&self, size: usize, type_desc_ptr: *const u8) -> *mut u8 {
        if self.options.assist_work_budget > 0 && self.check_is_marking_and_increment_busy() {
            self.do_mark_incremental(self.options.assist_work_budget);
            self.decrement_busy_marking();
        }

        let type_desc = type_desc_ptr.cast::<TypeDescriptor>();
        let obj = BeskidObject {
            heap: self as *const Self,
            type_desc,
            bytes: vec![0u8; size].into_boxed_slice(),
        };
        let ptr = GcBox::new_with_root(obj, false);
        let header_ptr = unsafe { &(*ptr.as_ptr()).header as *const GcHeader as *mut GcHeader };
        self.insert_allocation(header_ptr);

        let payload_ptr = unsafe { (*ptr.as_ptr()).data.bytes.as_ptr() as *mut u8 };
        if !type_desc_ptr.is_null() {
            // Keep existing Beskid runtime contract: type descriptor pointer is written
            // unaligned at the start of the allocation payload.
            let type_desc_addr = type_desc_ptr as usize;
            unsafe {
                std::ptr::write_unaligned(payload_ptr.cast::<usize>(), type_desc_addr);
            }
        }

        self.payload_to_header
            .lock()
            .insert(payload_ptr as usize, header_ptr as usize);
        self.header_to_payload
            .lock()
            .insert(header_ptr as usize, payload_ptr as usize);

        payload_ptr
    }

    fn insert_allocation(&self, header_ptr: *mut GcHeader) {
        let size = unsafe { (*header_ptr).vtable.layout.size() };
        loop {
            let current_head = self.head.load(Ordering::Acquire);
            unsafe {
                (*header_ptr).next.store(current_head, Ordering::Relaxed);
            }

            if self
                .head
                .compare_exchange(
                    current_head,
                    header_ptr,
                    Ordering::Release,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break;
            }
        }
        self.bytes_allocated.fetch_add(size, Ordering::Relaxed);
    }

    pub fn external_roots(&self) -> &ExternalRootSet {
        &self.external_roots
    }

    fn update_threshold(&self, live_bytes: usize) {
        let old_threshold = self.current_threshold.load(Ordering::Relaxed);
        let new_threshold = self.options.calculate_threshold(old_threshold, live_bytes);
        self.current_threshold
            .store(new_threshold, Ordering::Relaxed);
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

    /// Mark object addressed by payload pointer if it belongs to this heap.
    pub fn mark_payload_ptr(&self, payload_ptr: *mut u8, tracer: &Tracer) {
        if payload_ptr.is_null() {
            return;
        }
        let header = self
            .payload_to_header
            .lock()
            .get(&(payload_ptr as usize))
            .copied();
        if let Some(header_ptr) = header {
            // SAFETY: header pointers are inserted at allocation and removed on sweep.
            unsafe {
                tracer.mark_header(&*(header_ptr as *const GcHeader));
            }
        }
    }

    /// Dijkstra insertion barrier for raw Beskid payload pointers.
    pub fn write_barrier(&self, _dst_obj: *mut u8, value_ptr: *mut u8) {
        if value_ptr.is_null() || !self.check_is_marking_and_increment_busy() {
            return;
        }
        let tracer = Tracer::new();
        self.mark_payload_ptr(value_ptr, &tracer);
        if tracer.has_work() {
            self.merge_work(&tracer);
        }
        self.decrement_busy_marking();
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

    /// Check if GC is currently in marking phase
    pub fn is_marking(&self) -> bool {
        self.gc_phase() == GcPhase::Marking
    }

    pub fn gc_phase(&self) -> GcPhase {
        GcPhase::from(self.phase.load(Ordering::Acquire))
    }

    pub fn collection_threshold(&self) -> usize {
        self.current_threshold.load(Ordering::Relaxed)
    }

    pub fn external_root_count(&self) -> usize {
        self.external_roots.root_count()
    }

    pub fn stats(&self) -> GcHeapStats {
        GcHeapStats {
            bytes_allocated: self.bytes_allocated(),
            object_count: self.allocation_count(),
            phase: self.gc_phase(),
            collection_threshold: self.collection_threshold(),
            external_root_count: self.external_root_count(),
        }
    }

    pub fn check_is_marking_and_increment_busy(&self) -> bool {
        self.busy_marking_count.fetch_add(1, Ordering::AcqRel);
        if self.is_marking() {
            true
        } else {
            self.busy_marking_count.fetch_sub(1, Ordering::AcqRel);
            false
        }
    }

    pub fn decrement_busy_marking(&self) {
        self.busy_marking_count.fetch_sub(1, Ordering::AcqRel);
    }

    /// Try to transition to marking phase
    fn try_start_marking(&self) -> bool {
        self.phase
            .compare_exchange(
                GcPhase::Idle as u8,
                GcPhase::Marking as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Transition to sweeping phase
    fn start_sweeping(&self) {
        self.phase.store(GcPhase::Sweeping as u8, Ordering::Release);
    }

    /// Transition back to idle phase
    fn finish_gc(&self) {
        self.phase.store(GcPhase::Idle as u8, Ordering::Release);
    }

    pub(crate) fn try_mark_full(&self) -> bool {
        if !self.try_start_marking() {
            return false;
        }

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
        let live_bytes = self.do_sweep();
        self.update_threshold(live_bytes);
        self.finish_gc();
        live_bytes
    }

    /// Steal work from the shared gray queue into a tracer
    ///
    /// Returns true if work was stolen, false if queue is empty
    fn steal_work(&self, tracer: &Tracer, max_items: usize) -> bool {
        let mut gray_queue = self.gray_queue.lock();
        tracer.steal_from(max_items, &mut gray_queue.0)
    }

    /// Merge tracer's local work back to the shared gray queue
    pub(crate) fn merge_work(&self, tracer: &Tracer) {
        let mut gray_queue = self.gray_queue.lock();
        tracer.append_to(&mut gray_queue.0);
    }

    /// Process marking work using a tracer
    ///
    /// Steals work, processes it locally, then merges new work back
    fn do_mark_with_tracer(&self, tracer: &Tracer, work_budget: usize) -> usize {
        let mut work_done = 0;

        while work_done < work_budget {
            // Try to get work from tracer's local queue first
            let ptr = if let Some(p) = tracer.pop_work() {
                p
            } else {
                // Local queue empty, try to steal from shared queue
                const BATCH_SIZE: usize = 8;
                if !self.steal_work(tracer, BATCH_SIZE) {
                    // No work available anywhere
                    break;
                }
                continue;
            };

            // Process one object
            unsafe {
                let header = &*ptr;
                (header.vtable.trace)(ptr, tracer);
                header.mark_black();
            }

            work_done += 1;
        }

        // Merge any newly discovered work back to shared queue
        if tracer.has_work() {
            self.merge_work(tracer);
        }

        work_done
    }

    /// Perform a bounded amount of incremental marking work
    ///
    /// Returns true if marking is complete, false if more work remains
    fn do_mark_incremental(&self, work_budget: usize) -> bool {
        let tracer = Tracer::new();
        let work_done = self.do_mark_with_tracer(&tracer, work_budget);

        // If we did no work, marking is complete
        work_done == 0
    }

    fn yield_once_if_marking_busy(&self) -> bool {
        if self.busy_marking_count.load(Ordering::Acquire) > 0 {
            std::thread::yield_now();
            true
        } else {
            false
        }
    }

    fn do_mark_work_full(&self, tracer: &Tracer) {
        // Process until all work is complete
        while self.do_mark_with_tracer(tracer, self.options.incremental_work_budget) > 0
            || self.yield_once_if_marking_busy()
        {
            // Keep going until no more work
        }
    }

    fn do_mark_roots(&self, tracer: &Tracer) {
        // Walk the linked list to find roots
        let mut current = self.head.load(Ordering::Acquire);
        while !current.is_null() {
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

        // Merge roots into shared gray queue
        self.merge_work(tracer);
    }

    fn do_sweep(&self) -> usize {
        self.start_sweeping();

        let mut freed = 0;

        unsafe {
            let mut current = self.head.load(Ordering::Acquire);
            let mut prev_next: *const AtomicPtr<GcHeader> = &self.head;

            while !current.is_null() {
                let header = &*current;
                let next = header.next.load(Ordering::Acquire);

                // Check if object should be collected
                if header.is_white() {
                    if let Some(payload) = self.header_to_payload.lock().remove(&(current as usize))
                    {
                        self.payload_to_header.lock().remove(&payload);
                    }
                    // Remove from list by updating previous node's next pointer
                    (*prev_next).store(next, Ordering::Release);

                    // Get size from vtable and call drop function
                    let size = header.vtable.layout.size();
                    (header.vtable.drop)(current); // Proper Drop via Box::from_raw!
                    freed += size;

                    // Move to next, keeping same prev
                    current = next;
                } else {
                    // Reset color for next cycle
                    header.reset_white();

                    // Move both forward
                    prev_next = &header.next;
                    current = next;
                }
            }
        }

        self.bytes_allocated.fetch_sub(freed, Ordering::Relaxed) - freed
    }

    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated.load(Ordering::Relaxed)
    }

    pub fn allocation_count(&self) -> usize {
        let mut count = 0;
        let mut current = self.head.load(Ordering::Acquire);

        while !current.is_null() {
            count += 1;
            unsafe {
                current = (*current).next.load(Ordering::Acquire);
            }
        }

        count
    }

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

impl Drop for Heap {
    fn drop(&mut self) {
        let mut current = self.head.load(Ordering::Acquire);

        while !current.is_null() {
            unsafe {
                let header = &*current;
                let next = header.next.load(Ordering::Acquire);

                // Use vtable drop for proper Drop semantics
                (header.vtable.drop)(current);

                current = next;
            }
        }
    }
}

/// Background GC thread that performs incremental marking and sweeping
fn background_gc_thread(heap: Arc<Heap>, c: StopCondition) {
    let tracer = Tracer::new();
    while !heap.options.collection_interval.is_zero()
        && !heap
            .bg_thread
            .wait_stopped(c, heap.options.collection_interval)
    {
        // Check if we should start a collection
        if heap.should_collect() && heap.try_start_marking() {
            // STW pause: scan roots
            heap.do_mark_roots(&tracer);

            // Incremental marking phase
            loop {
                if heap.bg_thread.is_stopped(c) {
                    heap.finish_gc();
                    return;
                }

                let marking_complete =
                    heap.do_mark_incremental(heap.options.incremental_work_budget);
                if marking_complete {
                    if !heap.yield_once_if_marking_busy() {
                        break;
                    }
                } else {
                    // Yield to allow mutators to make progress
                    std::thread::yield_now();
                }
            }

            // Sweeping phase and finish
            heap.sweep_and_finish();
        }
    }
}
