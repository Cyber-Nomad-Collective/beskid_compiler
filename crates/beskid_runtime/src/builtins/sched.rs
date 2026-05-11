use std::sync::OnceLock;
use std::time::Instant;

/// Cooperative scheduler hint (`std::thread::yield_now`).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn rt_yield() {
    // Cooperative yield hint (optional, v0.1).
    std::thread::yield_now();
}

/// Monotonic milliseconds since first call in the process (saturated at `i64::MAX`).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn rt_now_millis() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed().as_millis();
    elapsed.min(i64::MAX as u128) as i64
}
