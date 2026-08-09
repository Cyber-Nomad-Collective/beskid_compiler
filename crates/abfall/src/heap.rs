//! Heap management and object storage.
//!
//! `Heap` is the sole coordinator for allocation ownership, collector state, collection work,
//! and the background lifecycle. Focused descendants keep each mechanism single-purpose while
//! preserving the established `abfall::heap` public API.

mod allocation;
mod background;
mod collection;
mod model;
mod registry;
mod state;

pub use allocation::BeskidAllocation;
pub use model::{GcHeapStats, GcOptions, GcPhase};

use background::StartStopJoinHandle;
use model::gc_state;
use registry::BeskidAllocationRegistry;
use state::GrayQueue;

use crate::gc_box::GcHeader;
use crate::roots::ExternalRootSet;
use std::ptr::null_mut;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// The garbage collected heap.
///
/// Manages allocation and deallocation of GC objects using an intrusive linked list, and
/// coordinates the mark and sweep collector with incremental marking support.
pub struct Heap {
    /// Head of the intrusive linked list of allocations.
    head: AtomicPtr<GcHeader>,
    /// Garbage collection options.
    options: GcOptions,
    /// Total bytes currently allocated.
    bytes_allocated: AtomicUsize,
    /// Current collection threshold in bytes.
    current_threshold: AtomicUsize,
    /// Gray queue for incremental marking.
    gray_queue: parking_lot::Mutex<GrayQueue>,
    /// Current GC phase plus a collection epoch, preventing an ABA phase observation.
    phase_state: AtomicUsize,
    /// Background GC thread handle.
    bg_thread: StartStopJoinHandle,
    /// Number of assist mutators or write-barriers active.
    busy_marking_count: std::sync::atomic::AtomicUsize,
    /// Mutator transactions that may publish roots or marking work.
    active_mutator_count: AtomicUsize,
    /// Runtime-registered roots and temporary handles.
    external_roots: ExternalRootSet,
    /// Ownership and pointer-to-header lookup for opaque Beskid payloads.
    beskid_allocations: BeskidAllocationRegistry,
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
            phase_state: AtomicUsize::new(gc_state(GcPhase::Idle, 0)),
            bg_thread: StartStopJoinHandle::new(),
            busy_marking_count: std::sync::atomic::AtomicUsize::new(0),
            active_mutator_count: AtomicUsize::new(0),
            external_roots: ExternalRootSet::default(),
            beskid_allocations: BeskidAllocationRegistry::default(),
        });

        heap.start_background_collection();

        heap
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        let mut current = self.head.load(Ordering::Acquire);

        while !current.is_null() {
            // SAFETY: `Heap::drop` has exclusive access to the heap after the background handle
            // has been dropped. Every list node is live until its vtable drop consumes it here.
            unsafe {
                let header = &*current;
                let next = header.next.load(Ordering::Acquire);

                (header.vtable.drop)(current);

                current = next;
            }
        }
    }
}
