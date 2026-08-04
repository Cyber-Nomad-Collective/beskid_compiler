use std::sync::Arc;

use abfall::{Heap, TypeDescriptor};
use beskid_runtime::{
    RuntimeRoot, alloc, array_new, clear_current_heap, clear_current_root, dynamic_cell_create, enter_runtime_scope,
    force_collect, gc_bytes_allocated,
    gc_collect, gc_collect_if_needed, gc_external_root_count, gc_object_count, gc_phase, gc_register_root,
    gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier, leave_runtime_scope, set_current_heap,
    set_current_root, snapshot_gc, str_concat, str_from_i64,
};

fn with_runtime_scope<R>(f: impl FnOnce(&Arc<Heap>, &mut RuntimeRoot) -> R) -> R {
    let heap = Heap::off();
    let mut root = RuntimeRoot::new(heap.clone());

    enter_runtime_scope();
    set_current_heap(&heap);
    set_current_root(&mut root as *mut _);
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            clear_current_heap();
            clear_current_root();
            leave_runtime_scope();
        }
    }
    let _guard = Guard;
    f(&heap, &mut root)
}

#[test]
fn alloc_remains_live_until_root_handoff_then_is_reclaimed() {
    with_runtime_scope(|heap, _| {
        let ptr = alloc(32, std::ptr::null());
        assert!(!ptr.is_null());
        let before_collect = heap.bytes_allocated();
        assert!(before_collect > 0);
        heap.force_collect();
        assert!(
            heap.owns_beskid_payload(ptr),
            "raw ABI allocation must retain its construction root before the caller can install a root"
        );

        let handle = gc_root_handle(ptr);
        gc_unroot_handle(handle);
        let after = heap.force_collect();
        assert!(after < before_collect, "root handoff must release the construction root");
    });
}

#[test]
fn array_composite_reclaims_header_and_buffer_after_outer_handoff() {
    with_runtime_scope(|heap, _| {
        let array = array_new(1, 16);
        let bytes_after_build = heap.bytes_allocated();
        let handle = gc_root_handle(array.cast());
        heap.force_collect();
        assert_eq!(heap.bytes_allocated(), bytes_after_build, "rooted array must trace its buffer");

        gc_unroot_handle(handle);
        assert!(heap.force_collect() < bytes_after_build, "unreachable array must release its buffer too");
        assert_eq!(heap.bytes_allocated(), 0, "array composite must not retain a construction root");
    });
}

#[test]
fn string_composite_reclaims_header_and_buffer_after_outer_handoff() {
    with_runtime_scope(|heap, _| {
        let value = str_from_i64(42);
        let bytes_after_build = heap.bytes_allocated();
        let handle = gc_root_handle(value.cast());
        heap.force_collect();
        assert_eq!(heap.bytes_allocated(), bytes_after_build, "rooted string must trace its buffer");

        gc_unroot_handle(handle);
        assert!(heap.force_collect() < bytes_after_build, "unreachable string must release its buffer too");
        assert_eq!(heap.bytes_allocated(), 0, "string composite must not retain a construction root");
    });
}

#[test]
fn concatenated_string_reclaims_all_managed_buffers_after_handoffs() {
    with_runtime_scope(|heap, _| {
        let left = str_from_i64(12);
        let right = str_from_i64(34);
        let combined = str_concat(left, right);
        let bytes_after_build = heap.bytes_allocated();
        let left_handle = gc_root_handle(left.cast());
        let right_handle = gc_root_handle(right.cast());
        let combined_handle = gc_root_handle(combined.cast());
        heap.force_collect();
        assert_eq!(heap.bytes_allocated(), bytes_after_build, "all rooted strings must trace their buffers");

        gc_unroot_handle(left_handle);
        gc_unroot_handle(right_handle);
        gc_unroot_handle(combined_handle);
        assert!(heap.force_collect() < bytes_after_build, "unreachable concatenation must release every buffer");
        assert_eq!(heap.bytes_allocated(), 0, "str_concat must not retain any construction roots");
    });
}

#[test]
fn dynamic_composite_reclaims_cell_and_payload_after_outer_handoff() {
    with_runtime_scope(|heap, _| {
        let payload = alloc(16, std::ptr::null());
        let cell = dynamic_cell_create(7, payload);
        let bytes_after_build = heap.bytes_allocated();
        let handle = gc_root_handle(cell.cast());
        heap.force_collect();
        assert_eq!(heap.bytes_allocated(), bytes_after_build, "rooted dynamic cell must trace its payload");

        gc_unroot_handle(handle);
        assert!(heap.force_collect() < bytes_after_build, "unreachable dynamic cell must release its payload too");
        assert_eq!(heap.bytes_allocated(), 0, "dynamic composite must not retain a construction root");
    });
}

#[test]
fn registered_root_and_handle_keep_object_alive() {
    with_runtime_scope(|heap, _| {
        let ptr = alloc(32, std::ptr::null());
        let mut slot = ptr;
        gc_register_root(&mut slot as *mut *mut u8);
        let handle = gc_root_handle(ptr);

        let before = heap.bytes_allocated();
        heap.force_collect();
        let during = heap.bytes_allocated();
        assert!(during >= before, "registered roots should keep payload alive");

        gc_unroot_handle(handle);
        gc_unregister_root(&mut slot as *mut *mut u8);
        let after = heap.force_collect();
        assert!(after < during, "removing roots should allow reclamation");
    });
}

#[test]
fn write_barrier_marks_newly_linked_child_during_marking() {
    let pointer_offsets = [8usize];
    let desc = TypeDescriptor {
        size: 24,
        align: 8,
        pointer_count: 1,
        pointer_offsets: pointer_offsets.as_ptr(),
        name: std::ptr::null(),
    };

    with_runtime_scope(|heap, _| {
        let parent = alloc(24, (&desc as *const TypeDescriptor).cast::<u8>());
        let child = alloc(24, std::ptr::null());
        let mut parent_slot = parent;
        gc_register_root(&mut parent_slot as *mut *mut u8);

        let before = heap.bytes_allocated();
        assert!(heap.mark_for_tests(), "expected mark phase to start");

        // Store child pointer into parent after root scan, then invoke barrier.
        unsafe {
            std::ptr::write_unaligned(parent.add(8).cast::<*mut u8>(), child);
        }
        gc_write_barrier(parent, child);

        heap.sweep_for_tests();
        let after = heap.bytes_allocated();
        assert!(after >= before, "child should survive when linked during marking with barrier");

        gc_unregister_root(&mut parent_slot as *mut *mut u8);
        heap.force_collect();
    });
}

#[test]
fn runtime_snapshot_reports_active_heap_state() {
    assert!(snapshot_gc().is_none());

    with_runtime_scope(|heap, _| {
        let _ptr = alloc(32, std::ptr::null());
        let snapshot = snapshot_gc().expect("runtime scope should expose a GC snapshot");

        assert_eq!(snapshot.bytes_allocated, heap.bytes_allocated());
        assert_eq!(snapshot.object_count, heap.allocation_count());
        assert!(snapshot.to_string().contains("phase="));

        let live_bytes = force_collect().expect("force collect requires runtime scope");
        assert_eq!(live_bytes, heap.bytes_allocated());
    });
}

#[test]
fn gc_state_builtins_report_and_control_active_heap() {
    with_runtime_scope(|heap, _| {
        assert_eq!(gc_external_root_count(), 0);
        let ptr = alloc(32, std::ptr::null());
        let mut slot = ptr;
        gc_register_root(&mut slot as *mut *mut u8);

        assert_eq!(gc_bytes_allocated(), heap.bytes_allocated());
        assert_eq!(gc_object_count(), heap.allocation_count());
        assert_eq!(gc_external_root_count(), 1);
        assert_eq!(gc_phase(), 0);

        let live = gc_collect();
        assert_eq!(live, heap.bytes_allocated());
        assert_eq!(gc_collect_if_needed(), heap.bytes_allocated());

        gc_unregister_root(&mut slot as *mut *mut u8);
        assert_eq!(gc_external_root_count(), 0);
    });
}
