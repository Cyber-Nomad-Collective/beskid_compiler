`Core.Time` defines UTC civil time types and clock helpers backed by `__clock_realtime_nanos` and `__clock_monotonic_nanos`.

## Types

| Type | Module | Role |
| --- | --- | --- |
| `Instant` | `Core.Time.Instant` | Nanosecond tick in a clock domain (realtime or monotonic). |
| `Duration` | `Core.Time.Duration` | Elapsed span in nanoseconds. |
| `Date` | `Core.Time.Date` | Proleptic Gregorian UTC calendar date. |
| `TimeOfDay` | `Core.Time.TimeOfDay` | UTC time within a civil day. |
| `DateTime` | `Core.Time.DateTime` | UTC date and time-of-day pair. |
| `TimeError` | `Core.Time.TimeError` | Parse and range failures for civil helpers. |

## Clock functions

| Function | Behavior |
| --- | --- |
| `NowUtc() -> Instant` | Realtime nanoseconds since Unix epoch. |
| `MonotonicNow() -> Instant` | Monotonic nanoseconds since process start. |
| `FromMilliseconds(i64) -> Duration` | Builds a duration from whole milliseconds. |
| `FromNanoseconds(i64) -> Duration` | Builds a duration from nanoseconds. |
| `FromSeconds(i64) -> Duration` | Builds a duration from whole seconds. |

## UTC civil conversions

| Function | Behavior |
| --- | --- |
| `ToUtcDateTime(Instant) -> DateTime` | Splits a realtime instant into UTC civil fields. |
| `FromUtcDateTime(DateTime) -> Instant` | Combines UTC civil fields into a realtime instant. |
| `FormatIso8601Utc(DateTime) -> string` | Renders `YYYY-MM-DDTHH:MM:SSZ` (second precision). |
| `FormatDateIso(Date) -> string` | Renders `YYYY-MM-DD`. |
| `ParseIso8601Date(string) -> Result<Date, TimeError>` | Parses strict `YYYY-MM-DD`. |

Compare instants only within the same clock domain. Monotonic instants must not be converted with UTC civil helpers.
