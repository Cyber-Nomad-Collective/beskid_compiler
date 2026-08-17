`Core.Results` defines the standard **`Result<TValue, TError>`** enum used whenever a function can fail in a typed way.

## Type

```beskid
pub enum Result<TValue, TError> {
    Ok(TValue value),
    Error(TError error),
}
```

## Usage

- Return **`Result::Ok(...)`** for success and **`Result::Error(...)`** for failure.
- System modules (`Core.FS`, `Core.Process`, `Core.Syscall`, …) use this shape with domain-specific `TError` enums.
- This is the preferred alternative to string-only error channels for recoverable failure at API boundaries.

## Helpers

- **`IsOk`** / **`IsError`** — boolean predicates for branching without matching directly on the enum at every call site.
