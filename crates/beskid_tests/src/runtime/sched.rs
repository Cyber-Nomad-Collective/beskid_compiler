use beskid_runtime::{fiber_now_millis, fiber_yield, rt_now_millis, rt_yield};

#[test]
fn runtime_scheduler_now_millis_is_monotonic() {
    let first = rt_now_millis();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = rt_now_millis();
    assert!(second >= first, "expected monotonic scheduler clock");
}

#[test]
fn runtime_scheduler_yield_is_callable() {
    rt_yield();
}

#[test]
fn fiber_now_millis_is_monotonic() {
    let first = fiber_now_millis();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = fiber_now_millis();
    assert!(second >= first);
}

#[test]
fn fiber_yield_without_scheduler_is_callable() {
    fiber_yield();
}
