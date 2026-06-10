`Query.Operators` defines **`QueryState<T>`** and array-backed combinators over `T[]` windows.

## Type

```beskid
pub type QueryState<T> {
    T[] source,
    i64 index,
    i64 length,
    Core.Optional.Option<T> first,
}
```

## Operators

| Function | Role |
|----------|------|
| `FromArray<T>` | Build state from a source array (`index=0`, `length=Len(source)`). |
| `Where` | Drop the window when `predicate` is false at the current position. |
| `Select` | Project the current element using a `sample` value (staged projection). |
| `Take` / `Skip` | Bound or advance the logical window. |
| `Count` | Remaining element count (`length - index`). |
| `First` | Optional first element for the current window. |
| `ToList` | Materialize the remaining window into a new `T[]`. |
| `Any` / `All` | Scan the window for any / all elements equal to a reference value. |
| `OrderBy` | Sort an `i64` window ascending or descending (bubble sort, v1). |
| `CollectArray` | Returns `Len(ToList(state))`. |

## Policy

- Import `Core.Optional` for optional query results; `Query.Contracts` is removed.
- Prefer explicit `Collections.Array` helpers for production storage until query syntax lowering lands.
