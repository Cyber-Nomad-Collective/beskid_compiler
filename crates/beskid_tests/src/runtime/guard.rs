use crate::support::runtime::with_runtime_scope;
use beskid_runtime::{
    alloc, enter_runtime_scope, gc_register_root, gc_root_handle, gc_unregister_root, gc_write_barrier,
    leave_runtime_scope, snapshot_gc,
};

#[test]
fn runtime_write_barrier_is_safe_with_null_pointers() {
    with_runtime_scope(|_, _| {
        gc_write_barrier(std::ptr::null_mut(), std::ptr::null_mut());
    });
}

#[test]
fn runtime_register_unregister_root_are_accepted() {
    with_runtime_scope(|_, _| {
        let mut value = std::ptr::null_mut();
        let value_ptr = &mut value as *mut *mut u8;
        gc_register_root(value_ptr);
        gc_unregister_root(value_ptr);
    });
}

#[test]
fn runtime_alloc_panics_without_runtime_scope() {
    let result = std::panic::catch_unwind(|| {
        let _ = alloc(8, std::ptr::null());
    });
    assert!(result.is_err(), "expected alloc to panic without runtime scope");
}

#[test]
fn runtime_root_handle_panics_without_runtime_scope() {
    let result = std::panic::catch_unwind(|| {
        let _ = gc_root_handle(std::ptr::null_mut());
    });
    assert!(result.is_err(), "expected gc_root_handle to panic without runtime scope");
}

#[test]
fn runtime_scope_supports_nesting_but_rejects_underflow() {
    enter_runtime_scope();
    enter_runtime_scope();
    leave_runtime_scope();
    leave_runtime_scope();
    let underflow = std::panic::catch_unwind(|| {
        leave_runtime_scope();
    });
    assert!(underflow.is_err(), "expected scope underflow to panic");
}

#[test]
fn runtime_scope_exposes_gc_snapshot() {
    with_runtime_scope(|heap, _| {
        let _ptr = alloc(40, std::ptr::null());
        let snapshot = snapshot_gc().expect("runtime scope should expose GC stats");

        assert_eq!(snapshot.bytes_allocated, heap.bytes_allocated());
        assert_eq!(snapshot.object_count, heap.allocation_count());
        assert_eq!(snapshot.phase as usize, beskid_runtime::gc_phase());
        assert_eq!(snapshot.external_root_count, beskid_runtime::gc_external_root_count());
    });
}
