//! ABI status codes returned as `i64` from channel, fiber, hub, mutex, and wait-group builtins.

/// Channel / hub operation succeeded.
pub const STATUS_OK: i64 = 0;
/// Channel closed (send/receive after close, empty receive).
pub const STATUS_CLOSED: i64 = 1;
/// Operation cancelled (fiber cancel).
pub const STATUS_CANCELLED: i64 = 2;
/// Try-send / try-receive would block.
pub const STATUS_WOULD_BLOCK: i64 = 3;
/// Hub has no registered channels.
pub const STATUS_HUB_EMPTY: i64 = 4;
/// Hub registration limit exceeded.
pub const STATUS_HUB_LIMIT: i64 = 5;
/// Hub index not registered.
pub const STATUS_HUB_NOT_FOUND: i64 = 6;

/// Fiber join succeeded.
pub const FIBER_JOIN_OK: i64 = 0;
/// Fiber join: target was cancelled.
pub const FIBER_JOIN_CANCELLED: i64 = 1;
/// Fiber join: child panicked.
pub const FIBER_JOIN_PANICKED: i64 = 2;
/// Fiber join: stack overflow in child.
pub const FIBER_JOIN_STACK_OVERFLOW: i64 = 3;
/// Fiber join: still running (should not surface at ABI after blocking join).
pub const FIBER_JOIN_NOT_DONE: i64 = 4;

/// Mutex lock acquired.
pub const MUTEX_OK: i64 = 0;
/// Mutex try-lock would block.
pub const MUTEX_WOULD_BLOCK: i64 = 1;
