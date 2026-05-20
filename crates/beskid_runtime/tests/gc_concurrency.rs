use std::sync::Arc;

use abfall::Heap;
use beskid_runtime::{
    RuntimeRoot, alloc, channel_create, channel_receive, channel_send, clear_current_heap,
    clear_current_root, enter_runtime_scope, fiber_join, fiber_spawn, gc_bytes_allocated,
    gc_collect, leave_runtime_scope, run_closure_as_main, set_current_heap, set_current_root,
    status::{FIBER_JOIN_OK, STATUS_OK},
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

extern "C" fn allocate_in_fiber(_env: *mut u8) -> i64 {
    let ptr = alloc(32, std::ptr::null());
    if ptr.is_null() { 0 } else { 7 }
}

#[test]
fn fiber_allocation_is_visible_to_gc_builtins() {
    with_runtime_scope(|heap, _| {
        run_closure_as_main(|| {
            let child = fiber_spawn(allocate_in_fiber, std::ptr::null_mut());
            let mut out = 0i64;
            assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
            assert_eq!(out, 7);
            0
        });

        assert!(gc_bytes_allocated() > 0);
        let live = gc_collect();
        assert_eq!(live, heap.bytes_allocated());
    });
}

#[test]
fn channel_wait_path_survives_gc_collection() {
    with_runtime_scope(|heap, _| {
        run_closure_as_main(|| {
            let ch = channel_create(0, 0);
            assert_eq!(channel_send(ch, 99), STATUS_OK);
            let _ptr = alloc(64, std::ptr::null());
            let live = gc_collect();
            assert!(live <= gc_bytes_allocated());

            let mut out = 0i64;
            assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
            assert_eq!(out, 99);
            0
        });

        assert_eq!(gc_collect(), heap.bytes_allocated());
    });
}
