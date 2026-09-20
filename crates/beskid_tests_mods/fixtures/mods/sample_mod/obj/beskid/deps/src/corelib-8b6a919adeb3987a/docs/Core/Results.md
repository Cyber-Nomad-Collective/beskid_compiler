# Core.Results

`Core.Results` defines the standard `Result<TValue, TError>` enum for typed recoverable failures.

```beskid
pub enum Result<TValue, TError> {
    Ok(TValue value),
    Error(TError error),
}
```

## Helpers

- `Success<TValue, TError>(value)` constructs `Result::Ok(value)`.
- `Failure<TValue, TError>(error)` constructs `Result::Error(error)`.
- `IsOk` and `IsError` inspect the active variant.
- `Map<TValue, TNext, TError>(result, mapped)` replaces an `Ok` payload and preserves an `Error` payload exactly.

```beskid
Result<i64, string> original = Result::Ok(1);
Result<string, string> mapped = Results.Map<i64, string, string>(original, "one");
```

## Unit success

Operations with no success data use the ordinary `Result<unit, TError>` specialization. `Ok` retains its normal discriminant and carries `()`; it is never represented as a fabricated boolean.

```beskid
pub Result<unit, string> Save() {
    return Result::Ok(());
}

Result<unit, string> saved = Save();
match saved {
    Result::Ok(()) => Console.WriteLine("saved"),
    Result::Error(error) => Console.WriteLine(error),
};
```

## Error propagation

Match both variants and return the original typed error when the caller does not transform it.

```beskid
pub Result<string, Core.FS.FsError> Load(string path) {
    Result<string, Core.FS.FsError> opened = Core.FS.ReadAllText(path);
    return match opened {
        Result::Ok(content) => Result::Ok(content),
        Result::Error(error) => Result::Error(error),
    };
}
```

The compiler treats `Result<unit, E>` as a normal generic enum instantiation; there is no unit-only layout or lowering special case.
