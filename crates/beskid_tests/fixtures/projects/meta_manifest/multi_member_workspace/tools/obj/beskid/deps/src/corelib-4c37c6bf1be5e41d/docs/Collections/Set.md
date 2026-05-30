`Collections.Set` defines **`Set<T>`** with a logical **`count`** and a **`Contains`** helper whose semantics are intentionally narrow until real storage lands.

## Type

```beskid
pub type Set<T> {
    i64 count,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `New<T>() -> Set<T>` | Empty set (`count = 0`). |
| `Count<T>(Set<T> set) -> i64` | Returns `set.count`. |
| `Contains<T>(Set<T> set, T value) -> bool` | If `set.count < 1`, returns **`false`**. Otherwise returns **`value == value`** (always **`true`** for comparable values)—a placeholder until proper hashing/equality-backed membership exists. |
