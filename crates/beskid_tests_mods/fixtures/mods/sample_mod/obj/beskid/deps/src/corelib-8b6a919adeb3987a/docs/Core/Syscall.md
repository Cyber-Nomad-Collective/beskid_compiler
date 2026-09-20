`Core.Syscall` is the low-level surface for **descriptor-oriented** I/O.
The raw wrappers (`Write`, `Read`) stay available for direct fd calls, while
ergonomic wrappers (`WriteWith`, `ReadWith`) model request options with
discriminated unions.

## Functions

| Function | Behavior |
|----------|----------|
| `StdoutFd() / StdinFd() / StderrFd()` | Return **`1`**, **`0`**, **`2`** respectively. |
| `DefaultReadLimit()` | Returns default read size used by `ReadWith` when `ReadLimit::Default` is selected. |
| `ResolveDescriptorFd(Descriptor)` | Maps `Descriptor::Standard(...)` or `Descriptor::Raw(...)` into a concrete fd. |
| `ResolveReadLimit(ReadLimit)` | Maps `ReadLimit::UpTo(n)` or `ReadLimit::Default` into a concrete byte limit. |
| `Write(i64 fd, string data)` | Negative fd → **`InvalidFd`**. Otherwise **`__syscall_write`**; negative result → **`IoFailure`**. |
| `WriteWith(WriteRequest request)` | Resolves typed descriptor from request and forwards to `Write`. |
| `Read(i64 fd, i64 maxBytes)` | Only **`fd == StdinFd()`** is supported; other fds → **`UnsupportedReadFd`**. `maxBytes < 1` → **`InvalidReadLimit`**. Success wraps **`__syscall_read`** in **`Ok`**. |
| `ReadWith(ReadRequest request)` | Resolves typed descriptor + read limit and forwards to `Read`. |

## Companion types (Prelude re-exports)

- **`Core.Syscall.StandardStream`** — `Stdin`, `Stdout`, `Stderr` enum
- **`Core.Syscall.Descriptor`** — `Standard(StandardStream)` or `Raw(i64)` descriptor selector
- **`Core.Syscall.ReadLimit`** — `UpTo(i64)` or `Default` read-size selector
- **`Core.Syscall.SyscallError`** — `InvalidFd`, `UnsupportedReadFd`, `InvalidReadLimit`, `IoFailure`
- **`Core.Syscall.WriteRequest`** — typed write DTO (`descriptor` + `data`)
- **`Core.Syscall.ReadRequest`** — typed read DTO (`descriptor` + `limit`)

## Usage examples

```beskid
Core.Results.Result<i64, Core.Syscall.SyscallError> out =
    Core.Syscall.WriteWith(
        WriteRequest {
            descriptor: Descriptor::Standard(StandardStream::Stdout),
            data: "hello",
        },
    );
```

```beskid
Core.Results.Result<string, Core.Syscall.SyscallError> text =
    Core.Syscall.ReadWith(
        ReadRequest {
            descriptor: Descriptor::Standard(StandardStream::Stdin),
            limit: ReadLimit::Default,
        },
    );
```

## Contract

- Prefer these builtins over ad-hoc print symbols; ABI evolution tracks **`BESKID_RUNTIME_ABI_VERSION`** at the runtime layer.
- One primary type per file under **`Core/Syscall/`** keeps generated docs and diffs readable.
- Prefer typed descriptor and limit selectors (`Descriptor`, `ReadLimit`) for new code; keep raw fd wrappers for compatibility.
