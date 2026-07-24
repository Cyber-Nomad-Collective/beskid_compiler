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

## Result combinators

`IsOk` and `IsError` are the primary combinators for conditional branching on a `Result` without destructuring it. They work as free functions or method-style calls on a `Result` value.

```beskid
let res: Result<I32, String> = might_fail();

// Branch with IsOk / IsError — no match needed for simple cases.
if IsOk(res) {
    // res.value is in scope here; use it directly.
    Core.Debug.Trace("succeeded");
} else if IsError(res) {
    Core.Debug.Trace("failed: ${res.error}");
}
```

`IsOk` and `IsError` narrow the variant in their respective branches, so `res.value` and `res.error` are accessible without a full `match`. Use them when you only care about one side.

## Pattern matching

All `match` arms on a `Result` must cover both `Ok` and `Error`. The compiler enforces exhaustiveness, so you cannot forget the error path.

```beskid
pub fn describe(res: Result<I32, String>) -> String {
    match res {
        Result::Ok(value) => "got ${value}",
        Result::Error(err) => "failed: ${err}",
    }
}
```

You can nest pattern matching on the payload when `TValue` or `TError` is itself an enum:

```beskid
match parse_and_validate(input) {
    Result::Ok(ValidationResult::Pass(score)) => "passed with ${score}",
    Result::Ok(ValidationResult::Warn(msg))    => "ok but ${msg}",
    Result::Error(ParseError::Malformed(line)) => "bad input at line ${line}",
    Result::Error(ParseError::Empty)            => "input was empty",
}
```

## Returning from a function

Wrap the happy path in `Result::Ok(...)` and each failure path in `Result::Error(...)`. The caller decides how to recover.

```beskid
pub fn divide(numerator: I32, denominator: I32) -> Result<I32, String> {
    if denominator == 0 {
        return Result::Error("division by zero");
    }
    return Result::Ok(numerator / denominator);
}

// Call site — match on both outcomes.
let answer = divide(10, 2);
match answer {
    Result::Ok(q)  => Core.Debug.Trace("quotient: ${q}"),
    Result::Error(e) => Core.Debug.Trace("error: ${e}"),
}
```

## Match-based error handling

When a caller receives a `Result`, the canonical way to handle it is `match`. This forces both paths to be addressed at compile time.

```beskid
let opened = Core.FS.ReadAllText("/etc/config.json");
match opened {
    Result::Ok(content) => {
        let parsed = parse_config(content);
        apply_config(parsed);
    },
    Result::Error(fs_err) => {
        Core.Debug.Trace("cannot read config: ${fs_err}");
        load_defaults();
    },
}
```

For early-exit patterns, combine `match` with `return` so the error path bails out and the happy path continues inline:

```beskid
pub fn load_user(id: I32) -> Result<User, String> {
    let raw = Core.FS.ReadAllText("/data/users/${id}.json");
    let json_text = match raw {
        Result::Ok(text) => text,
        Result::Error(e) => return Result::Error("read failed: ${e}"),
    };
    // json_text is now a plain String; proceed with parsing.
    let user = parse_user(json_text);
    return Result::Ok(user);
}
```

## Composing multiple operations

Chain fallible calls by matching each intermediate result. Each step either produces a value for the next step or short-circuits with an error.

```beskid
pub fn ingest(path: String) -> Result<Stats, String> {
    let raw = match Core.FS.ReadAllText(path) {
        Result::Ok(text) => text,
        Result::Error(e) => return Result::Error("read: ${e}"),
    };

    let decoded = match Core.Encoding.Hex.Decode(raw) {
        Result::Ok(bytes) => bytes,
        Result::Error(e) => return Result::Error("hex decode: ${e}"),
    };

    let stats = compute_stats(decoded);
    return Result::Ok(stats);
}
```

When the error types differ across steps (e.g. `FsError` vs `String`), convert each one into a common error type — here `String` — so the return type stays uniform. This pattern is the idiomatic way to compose fallible operations until dedicated combinator support (e.g. `and_then`) lands in the language.
