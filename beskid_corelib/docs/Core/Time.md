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

## Usage examples

### Current UTC time

```beskid
now := Core.Time.NowUtc()
dt := Core.Time.ToUtcDateTime(now)!
fmt := Core.Time.FormatIso8601Utc(dt)
// => "2025-07-11T14:22:05Z"
```

### Duration arithmetic

```beskid
d := Core.Time.FromSeconds(90)
// d encodes 90_000_000_000 nanoseconds internally
m := Core.Time.FromMilliseconds(3500)
// m encodes 3_500_000_000 nanoseconds
```

### Format/parse roundtrip

```beskid
dateResult := Core.Time.ParseIso8601Date("2025-03-15")
match dateResult {
  Core.Result.Ok(d) => {
    formatted := Core.Time.FormatDateIso(d)  // => "2025-03-15"
  },
  Core.Result.Err(e) => {
    // handle parse error
  },
}
```

## Gotchas

- **Two clock domains, don't mix.** `NowUtc` produces realtime instants; `MonotonicNow` produces monotonic instants. Never pass a monotonic `Instant` to `ToUtcDateTime` — the result is meaningless. Only compare instants from the same source.
- **No timezone support.** All civil helpers operate in UTC only. There is no local-time conversion, no IANA timezone database, and no offset-aware types. If you need wall-clock time for a specific locale you must implement the offset yourself.
- **Second precision in formatted output.** `FormatIso8601Utc` truncates to whole seconds (`HH:MM:SS`), discarding sub-second nanoseconds. Use the raw `Instant` for higher precision.
