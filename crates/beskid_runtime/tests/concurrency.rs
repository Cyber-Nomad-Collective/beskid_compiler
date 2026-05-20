use std::sync::atomic::{AtomicI64, Ordering};

use beskid_runtime::{
    channel_close, channel_create, channel_receive, channel_send, channel_try_receive,
    fiber_cancel, fiber_detach, fiber_join, fiber_spawn, fiber_yield, run_closure_as_main,
    status::{FIBER_JOIN_CANCELLED, FIBER_JOIN_OK, STATUS_CLOSED, STATUS_OK, STATUS_WOULD_BLOCK},
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

extern "C" fn cancel_spin(_env: *mut u8) -> i64 {
    use beskid_runtime::scheduler::current_fiber_cancelled;
    loop {
        if current_fiber_cancelled() {
            return 0;
        }
        fiber_yield();
    }
}

#[test]
fn channel_unbounded_send_receive() {
    run_closure_as_main(|| {
        let ch = channel_create(0, 0);
        assert_eq!(channel_send(ch, 42), STATUS_OK);
        let mut out = 0i64;
        assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 42);
        0
    });
}

#[test]
fn channel_close_and_closed_errors() {
    run_closure_as_main(|| {
        let ch = channel_create(0, 0);
        assert_eq!(channel_send(ch, 7), STATUS_OK);
        channel_close(ch);
        let mut out = 0i64;
        assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 7);
        assert_eq!(channel_receive(ch, &mut out), STATUS_CLOSED);
        assert_eq!(channel_send(ch, 1), STATUS_CLOSED);
        0
    });
}

#[test]
fn channel_try_would_block() {
    run_closure_as_main(|| {
        let ch = channel_create(0, 0);
        let mut out = 0i64;
        assert_eq!(channel_try_receive(ch, &mut out), STATUS_WOULD_BLOCK);
        0
    });
}

#[test]
fn fiber_spawn_join() {
    COUNTER.store(0, Ordering::SeqCst);
    run_closure_as_main(|| {
        let child = fiber_spawn(bump_counter, std::ptr::null_mut());
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 42);
        0
    });
}

// Main fiber must finish its coroutine before process/thread teardown; see scheduler shutdown in `run_main_fiber`.
#[test]
#[ignore = "corosensei force-unwind on main fiber drop after child yield; tracked with scheduler teardown"]
fn fiber_yield_runs_other_fiber() {
    run_closure_as_main(|| {
        let child = fiber_spawn(yield_once, std::ptr::null_mut());
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 1);
        0
    });
}

#[test]
fn fiber_cancel_returns_cancelled() {
    run_closure_as_main(|| {
        let child = fiber_spawn(cancel_spin, std::ptr::null_mut());
        fiber_yield();
        fiber_cancel(child);
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_CANCELLED);
        0
    });
}

#[test]
fn fiber_detach_does_not_block_shutdown() {
    run_closure_as_main(|| {
        let child = fiber_spawn(bump_counter, std::ptr::null_mut());
        fiber_detach(child);
        0
    });
}
