`Core.ErrorHandling` provides a small, serializable error description type for user-facing messages, separate from module-specific error enums (for example `System.FS.FsError`).

## Types and functions

| Name | Signature / role |
|------|------------------|
| `ErrorInfo` | `type` with `string code`, `string message` |
| `NewErrorInfo` | `ErrorInfo NewErrorInfo(string code, string message)` — builds a value with both fields set |
| `IsErrorInfoEmpty` | `bool IsErrorInfoEmpty(ErrorInfo info)` — `true` when `code` and `message` are both empty |

## Policy

- Use structured enums for domain errors at module boundaries; use **`ErrorInfo`** when you need a generic code + message pair.
- Callers can treat "empty" `ErrorInfo` via `IsErrorInfoEmpty` when combining metadata from several sources.
