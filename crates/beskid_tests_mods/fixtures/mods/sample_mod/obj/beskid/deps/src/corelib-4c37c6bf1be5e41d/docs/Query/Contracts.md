`Query.Contracts` defines a simple **`Option`** type (currently carrying **`i64`** in the `Some` payload) and a forward-only **`Iterator`** contract.

## Option

```beskid
pub enum Option {
    Some(i64 value),
    None,
}
```

## Iterator contract

```beskid
pub contract Iterator {
    Option Next();
}
```

## Helpers

| Function | Behavior |
|----------|----------|
| `HasValue(Option value) -> bool` | `true` for `Some`, `false` for `None`. |

The **`Option` payload is an `i64` today**—this is a minimal stand-in until generic `Option<T>` is available. New code should not assume `Some` can carry arbitrary `T` without checking future language changes.
