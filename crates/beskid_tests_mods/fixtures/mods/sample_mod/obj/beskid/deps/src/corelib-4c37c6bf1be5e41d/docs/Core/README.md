The **Core** modules hold types that almost every library touches: **`Core.Results`** for success/failure discrimination, **`Core.ErrorHandling`** for lightweight metadata, and **`Core.String`** for UTF-8 helpers backed by builtins.

## Modules

| Module | Role |
|--------|------|
| [Core.Results](./Results.md) | `Result<TValue, TError>` enum |
| [Core.ErrorHandling](./Error-Handling.md) | `ErrorInfo` and constructors |
| [Core.String](./String.md) | `Len`, `IsEmpty`, `Contains` |

## Conventions

- Public APIs use `PascalCase`.
- Prefer **`Result<_, _>`** for recoverable failures at module boundaries where the sources already do so.

## Entry point

`Prelude.bd` re-exports all three modules so typical programs see **`Core.Results`**, **`Core.ErrorHandling`**, and **`Core.String`** without extra module imports.
