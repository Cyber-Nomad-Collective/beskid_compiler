`Collections.List` exposes **`List<T>`** with a logical **`count`** field. Indexed access is present but storage is not implemented end-to-end.

## Type

```beskid
pub type List<T> {
    i64 count,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `New<T>() -> List<T>` | Empty list (`count = 0`). |
| `Count<T>(List<T> list) -> i64` | Returns `list.count`. |
| `Get<T>(List<T> list, i64 index) -> Result<T, string>` | Negative index → **`Result::Error("index out of range")`**. If `list.count > index`, returns **`Result::Error("list storage is not implemented")`**. Otherwise **`Result::Error("index out of range")`**. |

Until backing storage exists, treat **`List`** as a contract placeholder for APIs that will grow into full sequences.
