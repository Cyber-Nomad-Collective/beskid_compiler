# Core.Optional

`Core.Optional` provides `Option<T>` — the standard sum type for values that may or may not be present. Use `Option::Some(value)` when you have a value and `Option::None` when you don't. This replaces null references and sentinel-return conventions across all corelib APIs.

## Type

```beskid
pub enum Option<T> {
    Some(T value),
    None,
}
```

## Functions

| Function | Signature | Behavior |
|----------|-----------|----------|
| `HasValue` | `bool HasValue<T>(Option<T> value)` | Returns `true` for `Some`, `false` for `None`. |
| `UnwrapOr` | `T UnwrapOr<T>(Option<T> value, T defaultValue)` | Returns the inner value or `defaultValue` when `None`. |

## Basic usage

```beskid
Option<string> name = Option::Some("Beskid");
Option<string> missing = Option::None;

if HasValue(name) {
    // name is Some — safe to operate
}
```

## Pattern matching

```beskid
string display = match name {
    Option::Some(v) => v,
    Option::None => "unknown",
};
```

## Defaulting with UnwrapOr

```beskid
string title = UnwrapOr(missing, "untitled");
// title == "untitled"
```

## Common patterns

**Returning optional from a lookup:**

```beskid
Option<string> FindById(i64 id) {
    if id < 0 {
        return Option::None;
    }
    return Option::Some("item-${id}");
}
```

**Chaining with match (no `map`/`and_then` yet):**

```beskid
Option<string> full = match FindById(1) {
    Option::Some(v) => Option::Some("prefix-${v}"),
    Option::None => Option::None,
};
```

**Used across corelib:** `Core.Environment.TryGet` returns `Option<string>`, `Core.Text.Regex.Match` returns `Option<MatchSpan>`, and `Query.Operators.First` returns `Option<T>`.

## Gotchas

- `Option<T>` is not `Result<T, E>`. Use `Option` for presence/absence; use `Result` when you need to carry an error value.
- There is no `Unwrap` that panics on `None` — always provide a default via `UnwrapOr` or handle both variants via `match`.
