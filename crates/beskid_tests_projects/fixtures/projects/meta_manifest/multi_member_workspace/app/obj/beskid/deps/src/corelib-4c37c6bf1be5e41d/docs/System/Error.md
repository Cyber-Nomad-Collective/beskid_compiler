`System.Error` mirrors **`System.Output`** for **stderr** (`StandardStream::Stderr`).

## Functions

| Function | Behavior |
|----------|----------|
| `Write(string text)` | Writes UTF-8 `text` to stderr (no newline). |
| `WriteLine(string text)` | **`Write(text)`** then **`Write("\n")`**. |

Failures from `WriteWith` trigger **`__panic_str("System.Error.Write failed")`**.
