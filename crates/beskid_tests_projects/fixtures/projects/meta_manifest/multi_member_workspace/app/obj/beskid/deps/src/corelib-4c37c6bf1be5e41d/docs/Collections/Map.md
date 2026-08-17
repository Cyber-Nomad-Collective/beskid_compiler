`Collections.Map` defines **`Map<TKey, TValue>`**, **`MapEntry<TKey, TValue>`**, and counting helpers. Persistent storage and insertion APIs are not yet present in the checked-in sources—only empty maps and **`Count`**.

## Types

```beskid
pub type MapEntry<TKey, TValue> {
    TKey key,
    TValue value,
}

pub type Map<TKey, TValue> {
    i64 count,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `New<TKey, TValue>() -> Map<TKey, TValue>` | Map with `count = 0`. |
| `Count<TKey, TValue>(Map<TKey, TValue> map) -> i64` | Returns `map.count`. |
