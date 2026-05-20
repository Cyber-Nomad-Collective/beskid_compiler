use beskid_runtime::{fiber_now_millis, fiber_yield};

#[test]
fn fiber_now_millis_is_monotonic() {
    let first = fiber_now_millis();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = fiber_now_millis();
    assert!(second >= first, "expected monotonic scheduler clock");
}

#[test]
fn fiber_yield_without_scheduler_is_callable() {
    fiber_yield();
}
