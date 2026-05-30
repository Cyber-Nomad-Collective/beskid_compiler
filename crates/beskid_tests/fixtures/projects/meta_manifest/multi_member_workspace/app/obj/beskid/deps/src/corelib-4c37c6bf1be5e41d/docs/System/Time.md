`System.Time` defines **`Instant`** and **`Duration`** value types.

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
| `NowUtc() -> Instant` | Returns **`Instant { ticks: 0 }`**. |
| `MonotonicNow() -> Instant` | Returns **`Instant { ticks: 0 }`**. |
| `FromMilliseconds(i64 ms) -> Duration` | Returns **`Duration { milliseconds: ms }`**. |

Real wall-clock and monotonic reads require runtime support; the API shape is stable for forward use.
