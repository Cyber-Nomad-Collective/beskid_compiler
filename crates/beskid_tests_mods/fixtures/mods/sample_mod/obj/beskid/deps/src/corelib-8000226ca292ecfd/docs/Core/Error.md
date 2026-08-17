`Core.Error` mirrors **`Core.Output`** for **stderr** (`StandardStream::Stderr`).

## Functions

| Function | Behavior |
|----------|----------|
| `Write(string text)` | Writes UTF-8 `text` to stderr (no newline). |
| `WriteLine(string text)` | **`Write(text)`** then **`Write("\n")`**. |

Failures from `WriteWith` trigger **`__panic_str("Core.Error.Write failed")`**.
