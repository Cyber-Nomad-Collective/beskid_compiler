# Runtime Events Semantics (v1.0)

## Semantics
- Duplicates: allowed.
- Order: insertion order.
- Capacity: fixed per event state at first subscribe.
- Overflow: panic (`event capacity exceeded`).

## API Behavior
- `event_subscribe`: returns new length.
- `event_unsubscribe_first`: removes first matching handler; returns `1` on remove, `0` when not found.
- `event_len`: returns handler count (`0` for null state).
- `event_get_handler`: returns handler pointer or null when out of bounds.

## Iteration Policy
- Iteration is index-based over current handlers via `event_len + event_get_handler`.
- Mutation during iteration follows caller discipline in v1.0; handlers are read by index at dispatch time.
