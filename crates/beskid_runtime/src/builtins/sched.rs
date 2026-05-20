use crate::scheduler;

/// Deprecated: use [`super::fiber::fiber_yield`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn rt_yield() {
    scheduler::fiber_yield();
}

/// Deprecated: use [`super::fiber::fiber_now_millis`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn rt_now_millis() -> i64 {
    scheduler::fiber_now_millis()
}
