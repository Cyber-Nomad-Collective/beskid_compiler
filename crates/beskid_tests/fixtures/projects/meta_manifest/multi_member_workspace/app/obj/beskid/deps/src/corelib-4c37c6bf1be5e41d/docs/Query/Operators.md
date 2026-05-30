`Query.Operators` defines **`QueryState<T>`** and several operators that transform it. The current implementation is a **scaffold**: it uses `i64` counts and `Query.Contracts.Option` markers to approximate pipeline behavior.

## Type

```beskid
pub type QueryState<T> {
    i64 count,
    Query.Contracts.Option first,
}
```

## Operators (current behavior)

| Function | Role |
|----------|------|
| `Where` | If `predicate` is false, returns empty state; otherwise returns the input state. |
| `Select` | If `state.count < 1`, returns empty. Otherwise uses a self-equality check on `sample` to decide whether to carry `count` forward and set `first` to `Some(1)` (placeholder projection). |
| `Take` / `Skip` | Adjust `count` / `first` with simple boundary rules. |
| `Count` | Returns `state.count`. |
| `First` | Returns `Some(1)` or `None` based on `first` and `count` (not a true element payload yet). |
| `CollectArray` | Returns `state.count` as a stand-in for materialized length. |

## Policy

- Treat public names as stable; treat **semantics** as subject to change when real query execution lands.
- Prefer domain-specific collection code for production until the pipeline is runtime-backed.
