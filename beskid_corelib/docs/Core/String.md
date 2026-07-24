`Core.String` wraps the runtime string length builtin and provides small helpers for common checks.

## API

| Function | Behavior |
|----------|----------|
| `Len(string text) -> i64` | Returns `__str_len(text)` (UTF-8 code unit count in the current runtime representation). |
| `IsEmpty(string text) -> bool` | `Len(text) == 0`. |
| `Contains(string text, string needle) -> bool` | Returns `true` for an empty `needle`. If `Len(needle) > Len(text)` or the strings are exactly equal, returns `true` / `true` as appropriate. **Substrings are not fully implemented yet**—the current body does not scan general substrings, so do not rely on `Contains` for production substring search until the implementation is completed. |

## Policy

- Prefer explicit length or equality checks when `Contains` semantics are not yet required.
- `Testing.Assertions.AssertContains` calls `Core.String.Contains`; see [Testing.Assertions](../Testing/Assertions.md) for test expectations.

## Usage examples

```beskid
// Len check — bail early on short input
let username := "ab";
if String.Len(username) < 3 {
  return Error("username too short");
};
// > Error("username too short")
```

```beskid
// IsEmpty guard — skip work when nothing to process
let input := "";
if String.IsEmpty(input) {
  return "(empty)";
};
// > "(empty)"
```

## Gotchas

- **`Contains` is not fully implemented.** It handles empty needle, needle longer than text, and exact equality, but does not scan for general substrings yet. Do not rely on it for production substring search until the implementation is completed.
- **`Len` returns UTF-8 code units, not characters.** A single Unicode character may span multiple bytes (code units), so `Len("café")` may return 5 (4 ASCII + 1 two-byte é), not 4.
