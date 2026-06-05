`System.Output` implements **`Write`** and **`WriteLine`** using
**`System.Syscall.WriteWith`** and typed descriptor selection
(`Descriptor::Standard(StandardStream::Stdout)`). Failures from `WriteWith`
trigger **`__panic_str("System.Output.Write failed")`**.

## Functions

| Function | Behavior |
|----------|----------|
| `Write(string text)` | Writes UTF-8 `text` to stdout (no newline). |
| `WriteLine(string text)` | **`Write(text)`** then **`Write("\n")`**. |

## Contract

- Encoding follows the language **`string`** representation (UTF-8 bytes at the syscall boundary).
- Line endings are corelib policy (`"\n"`), not a separate runtime builtin.
