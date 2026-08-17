`System.Time` defines **`Instant`** and **`Duration`** value types with clock builtins.

## Types

```beskid
pub type Instant {
    i64 ticks,
}

pub type Duration {
    i64 milliseconds,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `NowUtc() -> Instant` | **`Instant { ticks: __clock_realtime_nanos() }`** (nanoseconds since Unix epoch). |
| `MonotonicNow() -> Instant` | **`Instant { ticks: __clock_monotonic_nanos() }`** (monotonic nanoseconds). |
| `FromMilliseconds(i64 ms) -> Duration` | Returns **`Duration { milliseconds: ms }`**. |

Tick values are raw nanoseconds from the host; compare durations in the same clock domain.
