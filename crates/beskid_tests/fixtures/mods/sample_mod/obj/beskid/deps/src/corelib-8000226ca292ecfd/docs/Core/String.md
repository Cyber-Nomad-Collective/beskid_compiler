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
