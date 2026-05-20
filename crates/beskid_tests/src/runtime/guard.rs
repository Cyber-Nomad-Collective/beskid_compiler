use beskid_engine::Engine;
use beskid_runtime::{
    alloc, enter_runtime_scope, gc_register_root, gc_root_handle, gc_unregister_root,
    gc_write_barrier, leave_runtime_scope,
};

#[test]
fn runtime_write_barrier_is_safe_with_null_pointers() {
    let mut engine = Engine::new();
    engine.with_runtime(|_, _| {
        gc_write_barrier(std::ptr::null_mut(), std::ptr::null_mut());
    });
}

#[test]
fn runtime_register_unregister_root_are_accepted() {
    let mut engine = Engine::new();
    engine.with_runtime(|_, _| {
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
    assert!(
        result.is_err(),
        "expected alloc to panic without runtime scope"
    );
}

#[test]
fn runtime_root_handle_panics_without_runtime_scope() {
    let result = std::panic::catch_unwind(|| {
        let _ = gc_root_handle(std::ptr::null_mut());
    });
    assert!(
        result.is_err(),
        "expected gc_root_handle to panic without runtime scope"
    );
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
