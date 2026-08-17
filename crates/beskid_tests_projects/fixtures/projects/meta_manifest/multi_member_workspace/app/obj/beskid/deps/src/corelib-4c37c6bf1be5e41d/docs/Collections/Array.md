`Collections.Array` defines **`ArrayIter<T>`** and iteration helpers over slice-like **`T[]`** values.

Slice-like arrays use the runtime **`BeskidArray`** layout; element count is read through the **`__array_len`** compiler builtin (not an OS syscall).

## Type

```beskid
pub type ArrayIter<T> {
    i64 index,
    i64 length,
}
```

## Functions

| Function | Behavior |
|----------|----------|
| `Len<T>(T[] values) -> i64` | Returns **`__array_len(values)`** (logical element count from the runtime header). |
| `Iterate<T>(T[] values) -> ArrayIter<T>` | Iterator at index `0` with `length = Len(values)`. |
| `HasNext<T>(ArrayIter<T> it) -> bool` | `it.index < it.length`. |

## Policy

- Prefer language-level array semantics for bounds; iterators stay consistent with **`Len`** because both read the same runtime length.
- **`Collections.Array`** is not re-exported from **`Prelude.bd`**; import the module explicitly when you need these helpers.
