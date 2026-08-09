use super::{GcHeapStats, Heap};
use crate::beskid::{AlignedBytes, ArrayElementDescriptor, BeskidArrayMetadata, BeskidObject, TypeDescriptor};
use crate::gc_box::{GcBox, GcHeader};
use crate::ptr::GcRoot;
use crate::roots::ExternalRootSet;
use crate::trace::{Trace, Tracer};
use std::sync::atomic::Ordering;

/// An opaque Beskid allocation kept rooted until it is published to the heap.
#[must_use = "an opaque Beskid allocation must be published or dropped"]
pub struct BeskidAllocation<'heap> {
    heap: &'heap Heap,
    payload_ptr: *mut u8,
}

impl BeskidAllocation<'_> {
    pub fn as_ptr(&self) -> *mut u8 {
        self.payload_ptr
    }

    pub fn publish(self) -> *mut u8 {
        self.heap.publish_raw_beskid(self.payload_ptr);
        let payload_ptr = self.payload_ptr;
        std::mem::forget(self);
        payload_ptr
    }

    pub fn into_raw_rooted(self) -> *mut u8 {
        let payload_ptr = self.payload_ptr;
        std::mem::forget(self);
        payload_ptr
    }
}

impl Drop for BeskidAllocation<'_> {
    fn drop(&mut self) {
        self.heap.release_construction_root(self.payload_ptr);
    }
}

impl Heap {
    pub fn allocate<T: Trace>(&self, data: T) -> GcRoot<T> {
        if self.options.assist_work_budget > 0 {
            self.with_mutator_operation(|is_marking| {
                if is_marking {
                    self.do_mark_incremental(self.options.assist_work_budget);
                }
            });
        }

        let _mutator = self.enter_mutator_operation();
        let ptr = GcBox::new(data);
        // SAFETY: `ptr` owns a fully initialized `GcBox<T>` whose header remains live after it is
        // linked into this heap. The cast only recovers the allocation's intrusive header.
        let header_ptr = unsafe { &(*ptr.as_ptr()).header as *const GcHeader as *mut GcHeader };
        self.insert_allocation(header_ptr);

        // SAFETY: `GcBox::new` returns a non-null rooted allocation, and the heap now owns it.
        unsafe { GcRoot::new_from_nonnull(ptr) }
    }

    /// Allocate an opaque Beskid payload tracked by descriptor metadata.
    pub fn allocate_beskid(&self, size: usize, type_desc_ptr: *const u8) -> BeskidAllocation<'_> {
        if self.options.assist_work_budget > 0 {
            self.with_mutator_operation(|is_marking| {
                if is_marking {
                    self.do_mark_incremental(self.options.assist_work_budget);
                }
            });
        }

        let _mutator = self.enter_mutator_operation();
        let type_desc = type_desc_ptr.cast::<TypeDescriptor>();
        let obj = BeskidObject { heap: self as *const Self, type_desc, bytes: AlignedBytes::zeroed(size), array: None };
        let ptr = GcBox::new_with_root(obj, true);
        // SAFETY: `ptr` owns a fully initialized `GcBox<BeskidObject>`.
        let header_ptr = unsafe { &(*ptr.as_ptr()).header as *const GcHeader as *mut GcHeader };
        self.insert_allocation(header_ptr);

        // SAFETY: `ptr` remains rooted and live; this obtains its stable, aligned payload base.
        let payload_ptr = unsafe { (*ptr.as_ptr()).data.bytes.as_ptr() as *mut u8 };
        if !type_desc_ptr.is_null() {
            // Keep existing Beskid runtime contract: type descriptor pointer is written
            // unaligned at the start of the allocation payload.
            let type_desc_addr = type_desc_ptr as usize;
            // SAFETY: the allocation has `size` bytes under the established ABI contract, whose
            // caller guarantees enough header storage for the descriptor word.
            unsafe {
                std::ptr::write_unaligned(payload_ptr.cast::<usize>(), type_desc_addr);
            }
        }

        self.beskid_allocations.register(payload_ptr, size, header_ptr, true);

        BeskidAllocation { heap: self, payload_ptr }
    }

    /// Allocate a typed array under a construction root before publishing it to the collector.
    ///
    /// The registry is installed before the external handle so a concurrent root scan can trace
    /// the object, and the intrusive allocation-list publication happens last. Callers must keep
    /// the returned handle until every pointer element has been written and barriered.
    pub fn allocate_beskid_array_constructing(
        &self,
        header_size: usize,
        descriptor: ArrayElementDescriptor,
        length: usize,
        initialize: impl FnOnce(*mut u8),
    ) -> Option<(*mut u8, u64)> {
        let data_size = descriptor.stride.checked_mul(length)?;
        let total_size = header_size.checked_add(data_size)?;
        if self.options.assist_work_budget > 0 {
            self.with_mutator_operation(|is_marking| {
                if is_marking {
                    self.do_mark_incremental(self.options.assist_work_budget);
                }
            });
        }
        let _mutator = self.enter_mutator_operation();
        let obj = BeskidObject {
            heap: self as *const Self,
            type_desc: std::ptr::null(),
            bytes: AlignedBytes::zeroed(total_size),
            array: Some(BeskidArrayMetadata { descriptor, length, data_offset: header_size }),
        };
        let ptr = GcBox::new_with_root(obj, false);
        // SAFETY: `ptr` owns a fully initialized `GcBox<BeskidObject>`.
        let header_ptr = unsafe { &(*ptr.as_ptr()).header as *const GcHeader as *mut GcHeader };
        // SAFETY: `ptr` is live and private until publication below.
        let payload_ptr = unsafe { (*ptr.as_ptr()).data.bytes.as_ptr() as *mut u8 };
        // Finish the ABI-visible header while this allocation is still private. A collector can
        // only observe the object after the external construction root and heap-list publication
        // below, so it never traces partially initialized header storage.
        initialize(payload_ptr);
        self.beskid_allocations.register(payload_ptr, total_size, header_ptr, false);
        // This handle must precede publication: a collector which reaches `head` can now also
        // reach the complete (zeroed) object through its construction root.
        let handle = self.external_roots.push_handle(payload_ptr);
        self.insert_allocation(header_ptr);
        Some((payload_ptr, handle))
    }

    fn insert_allocation(&self, header_ptr: *mut GcHeader) {
        // SAFETY: callers pass a newly allocated live header not yet linked into another heap.
        let size = unsafe { (*header_ptr).vtable.layout.size() };
        loop {
            let current_head = self.head.load(Ordering::Acquire);
            // SAFETY: the header is exclusively owned until the release CAS publishes it.
            unsafe {
                (*header_ptr).next.store(current_head, Ordering::Relaxed);
            }

            if self.head.compare_exchange(current_head, header_ptr, Ordering::Release, Ordering::Acquire).is_ok() {
                break;
            }
        }
        self.bytes_allocated.fetch_add(size, Ordering::Relaxed);
    }

    pub fn external_roots(&self) -> &ExternalRootSet {
        &self.external_roots
    }

    /// Whether this heap currently owns an opaque Beskid payload pointer.
    ///
    /// This is an ownership query only; it neither roots nor dereferences the payload.
    pub fn owns_beskid_payload(&self, payload_ptr: *mut u8) -> bool {
        self.beskid_allocations.owns(payload_ptr)
    }

    /// Mark object addressed by payload pointer if it belongs to this heap.
    pub fn mark_payload_ptr(&self, payload_ptr: *mut u8, tracer: &Tracer) {
        if payload_ptr.is_null() {
            return;
        }
        let header = self.beskid_allocations.header_for(payload_ptr);
        if let Some(header_ptr) = header {
            // SAFETY: header pointers are inserted at allocation and removed on sweep.
            unsafe {
                tracer.mark_header(&*header_ptr);
            }
        }
    }

    /// Dijkstra insertion barrier for raw Beskid payload pointers.
    pub fn write_barrier(&self, _dst_obj: *mut u8, value_ptr: *mut u8) {
        if value_ptr.is_null() {
            return;
        }
        self.with_mutator_operation(|is_marking| {
            if is_marking {
                let tracer = Tracer::new();
                self.mark_payload_ptr(value_ptr, &tracer);
                if tracer.has_work() {
                    self.merge_work(&tracer);
                }
            }
        });
    }

    /// Finish a raw ABI allocation hand-off after the caller has made it reachable.
    pub fn publish_raw_beskid(&self, payload_ptr: *mut u8) {
        self.write_barrier(std::ptr::null_mut(), payload_ptr);
        self.release_construction_root(payload_ptr);
    }

    /// Record a raw managed edge that has no descriptor-owned storage map.
    pub fn publish_composite_beskid_edge(&self, parent_ptr: *mut u8, child_ptr: *mut u8) {
        self.beskid_allocations.add_composite_edge(parent_ptr, child_ptr);
        self.write_barrier(parent_ptr, child_ptr);
        self.release_construction_root(child_ptr);
    }

    pub(crate) fn mark_composite_children(&self, parent_ptr: *mut u8, tracer: &Tracer) {
        for child_ptr in self.beskid_allocations.composite_children(parent_ptr) {
            self.mark_payload_ptr(child_ptr, tracer);
        }
    }

    fn release_construction_root(&self, payload_ptr: *mut u8) {
        if let Some(header_ptr) = self.beskid_allocations.take_construction_root(payload_ptr) {
            // SAFETY: the construction root keeps the registered allocation alive until this decrement.
            unsafe { (*header_ptr).dec_root() };
        }
    }

    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated.load(Ordering::Relaxed)
    }

    pub fn allocation_count(&self) -> usize {
        let mut count = 0;
        let mut current = self.head.load(Ordering::Acquire);

        while !current.is_null() {
            count += 1;
            // SAFETY: list nodes remain live while sweeping is excluded by the caller's ordinary
            // heap access protocol; acquire observes the published next link.
            unsafe {
                current = (*current).next.load(Ordering::Acquire);
            }
        }

        count
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
}
