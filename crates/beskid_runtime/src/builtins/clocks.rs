//! Monotonic and wall-clock nanosecond sources for scheduler and `System.Time`.

/// UTC wall-clock nanoseconds since Unix epoch (best effort).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn clock_realtime_nanos() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0)
}

/// Monotonic nanoseconds (best effort).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn clock_monotonic_nanos() -> i64 {
    use std::time::Instant;
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as i64
}
