# Runtime Scheduler and Time (feature = `sched`)

## Primitives
- `rt_yield()`: cooperative yield hint via `std::thread::yield_now`.
- `rt_now_millis()`: monotonic milliseconds since runtime process start.

## Guarantees
- `rt_now_millis()` is monotonic non-decreasing.
- Values are clamped to `i64::MAX`.

## Feature Gating
- Scheduler symbols are available only when runtime is built with `sched`.
