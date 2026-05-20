use std::sync::atomic::{AtomicI64, Ordering};

use beskid_runtime::{
    fiber_cancel, fiber_detach, fiber_join, fiber_spawn, fiber_yield, run_closure_as_main,
    status::{FIBER_JOIN_CANCELLED, FIBER_JOIN_OK},
};

static COUNTER: AtomicI64 = AtomicI64::new(0);

extern "C" fn bump_counter(_env: *mut u8) -> i64 {
    COUNTER.fetch_add(1, Ordering::SeqCst);
    42
}

extern "C" fn yield_once(_env: *mut u8) -> i64 {
    fiber_yield();
    1
}

extern "C" fn noop(_env: *mut u8) -> i64 {
    0
}

#[test]
fn fiber_spawn_join_returns_value() {
    COUNTER.store(0, Ordering::SeqCst);
    run_closure_as_main(|| {
        let child = fiber_spawn(bump_counter, std::ptr::null_mut());
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 42);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
        0
    });
}

#[test]
fn fiber_yield_runs_other_fiber() {
    run_closure_as_main(|| {
        let child = fiber_spawn(yield_once, std::ptr::null_mut());
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 1);
        0
    });
}

extern "C" fn cancel_once(_env: *mut u8) -> i64 {
    use beskid_runtime::scheduler::current_fiber_cancelled;
    fiber_yield();
    if current_fiber_cancelled() { 0 } else { 1 }
}

#[test]
fn fiber_cancel_join_returns_cancelled() {
    run_closure_as_main(|| {
        let child = fiber_spawn(cancel_once, std::ptr::null_mut());
        fiber_yield();
        fiber_cancel(child);
        fiber_yield();
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_CANCELLED);
        0
    });
}

#[test]
fn fiber_detach_skips_shutdown_join() {
    run_closure_as_main(|| {
        let child = fiber_spawn(noop, std::ptr::null_mut());
        fiber_detach(child);
        0
    });
}
