use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use beskid_runtime::{
    channel_close, channel_create, channel_receive, channel_send, channel_try_receive, channel_try_send, fiber_cancel,
    fiber_detach, fiber_join, fiber_spawn, fiber_spawn_with_cancel_slot, fiber_yield, hub_create, hub_register,
    hub_wait_receive, mutex_create, mutex_lock, mutex_try_lock, mutex_unlock, run_closure_as_main,
    status::{FIBER_JOIN_CANCELLED, FIBER_JOIN_OK, STATUS_CLOSED, STATUS_OK, STATUS_WOULD_BLOCK},
    wait_group_add, wait_group_create, wait_group_done, wait_group_wait,
};

static COUNTER: AtomicI64 = AtomicI64::new(0);
static ORDER: AtomicI64 = AtomicI64::new(0);

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

extern "C" fn wait_group_worker(env: *mut u8) -> i64 {
    let group = env as isize as i64;
    wait_group_done(group);
    11
}

extern "C" fn mutex_unlock_worker(env: *mut u8) -> i64 {
    let mutex = env as isize as i64;
    assert_eq!(mutex_lock(mutex), STATUS_OK);
    ORDER.store(2, Ordering::SeqCst);
    mutex_unlock(mutex);
    12
}

extern "C" fn parked_blocking_worker(_env: *mut u8) -> i64 {
    let fiber = beskid_runtime::scheduler::current_fiber_key().expect("fiber key");
    beskid_runtime::scheduler::run_blocking(fiber, || {
        std::thread::sleep(Duration::from_millis(50));
        77
    })
}

extern "C" fn progress_worker(_env: *mut u8) -> i64 {
    ORDER.store(3, Ordering::SeqCst);
    33
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
fn bounded_channel_reports_full_and_close() {
    run_closure_as_main(|| {
        let ch = channel_create(1, 0);
        assert_eq!(channel_try_send(ch, 10), STATUS_OK);
        assert_eq!(channel_try_send(ch, 11), STATUS_WOULD_BLOCK);

        let mut out = 0i64;
        assert_eq!(channel_receive(ch, &mut out), STATUS_OK);
        assert_eq!(out, 10);
        channel_close(ch);
        assert_eq!(channel_try_send(ch, 12), STATUS_CLOSED);
        assert_eq!(channel_receive(ch, &mut out), STATUS_CLOSED);
        0
    });
}

#[test]
fn hub_wait_receive_round_robins_ready_channels() {
    run_closure_as_main(|| {
        let first = channel_create(0, 0);
        let second = channel_create(0, 0);
        let hub = hub_create();
        assert_eq!(hub_register(hub, 10, first), STATUS_OK);
        assert_eq!(hub_register(hub, 20, second), STATUS_OK);
        assert_eq!(channel_send(first, 101), STATUS_OK);
        assert_eq!(channel_send(second, 202), STATUS_OK);

        let mut index = 0i64;
        let mut value = 0i64;
        assert_eq!(hub_wait_receive(hub, &mut index, &mut value), STATUS_OK);
        assert_eq!((index, value), (10, 101));
        assert_eq!(channel_send(first, 303), STATUS_OK);
        assert_eq!(hub_wait_receive(hub, &mut index, &mut value), STATUS_OK);
        assert_eq!((index, value), (20, 202));
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

#[test]
fn fiber_spawn_with_cancel_slot_returns_i64_handle() {
    run_closure_as_main(|| {
        let mut on_cancelled_slot = std::ptr::null_mut();
        let child = fiber_spawn_with_cancel_slot(bump_counter, std::ptr::null_mut(), &mut on_cancelled_slot);
        assert!(child > 0, "spawn should return an i64 fiber id");
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 42);
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

#[test]
fn wait_group_waits_for_worker_done() {
    run_closure_as_main(|| {
        let group = wait_group_create();
        wait_group_add(group, 1);
        let child = fiber_spawn(wait_group_worker, group as isize as *mut u8);
        assert_eq!(wait_group_wait(group), STATUS_OK);
        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 11);
        0
    });
}

#[test]
fn mutex_lock_parks_until_unlocked() {
    ORDER.store(0, Ordering::SeqCst);
    run_closure_as_main(|| {
        let mutex = mutex_create();
        assert_eq!(mutex_lock(mutex), STATUS_OK);
        let child = fiber_spawn(mutex_unlock_worker, mutex as isize as *mut u8);
        fiber_yield();
        assert_eq!(ORDER.load(Ordering::SeqCst), 0);
        mutex_unlock(mutex);

        let mut out = 0i64;
        assert_eq!(fiber_join(child, &mut out), FIBER_JOIN_OK);
        assert_eq!(out, 12);
        assert_eq!(ORDER.load(Ordering::SeqCst), 2);
        assert_eq!(mutex_try_lock(mutex), STATUS_OK);
        mutex_unlock(mutex);
        0
    });
}

#[test]
fn syscall_pool_parks_blocking_work_so_other_fibers_progress() {
    ORDER.store(0, Ordering::SeqCst);
    run_closure_as_main(|| {
        let blocked = fiber_spawn(parked_blocking_worker, std::ptr::null_mut());
        fiber_yield();
        let progress = fiber_spawn(progress_worker, std::ptr::null_mut());

        let mut progress_out = 0i64;
        assert_eq!(fiber_join(progress, &mut progress_out), FIBER_JOIN_OK);
        assert_eq!(progress_out, 33);
        assert_eq!(ORDER.load(Ordering::SeqCst), 3);

        let mut blocked_out = 0i64;
        assert_eq!(fiber_join(blocked, &mut blocked_out), FIBER_JOIN_OK);
        assert_eq!(blocked_out, 77);
        0
    });
}

#[test]
fn fiber_join_status_then_join_value_matches_single_join() {
    run_closure_as_main(|| {
        let child = fiber_spawn(bump_counter, std::ptr::null_mut());
        assert_eq!(beskid_runtime::fiber_join_status(child), FIBER_JOIN_OK);
        assert_eq!(beskid_runtime::fiber_join_value(child), 42);
        0
    });
}
