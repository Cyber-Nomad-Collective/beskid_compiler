**System** modules cover OS-facing concerns. The narrowest, best-supported path today is **`System.Syscall`** (fd reads/writes via builtins) and split stream helpers **`System.Input`**, **`System.Output`**, and **`System.Error`**.

Higher-level modules (`FS`, `Path`, `Environment`, `Process`, `Time`) largely expose **types and contracts** with **stub or placeholder** implementations until the runtime grows fuller platform support.

## Modules

| Module | Notes |
|--------|-------|
| [Syscall](./Syscall.md) | **`Write`**, **`Read`** backed by **`__syscall_write`** / **`__syscall_read`** |
| [Input](./Input.md) | **`Read`**, **`ReadLine`** → stdin via syscall |
| [Output](./Output.md) | **`Write`**, **`WriteLine`** → stdout via syscall |
| [Error](./Error.md) | **`Write`**, **`WriteLine`** → stderr via syscall |
| [FS](./FS.md) | **`FsError`**, **`ReadAllText`**, **`WriteAllText`**, **`Exists`** — file I/O returns errors except trivial cases |
| [Path](./Path.md) | **`Combine`** concatenates; **`FileName`** / **`Extension`** return the argument unchanged (stubs) |
| [Time](./Time.md) | **`Instant`**, **`Duration`**; **`NowUtc`** / **`MonotonicNow`** return zero ticks |
| [Environment](./Environment.md) | **`Get`** / **`TryGet`** / **`Set`** mostly errors or empty; **`CurrentDirectory`** returns `"."` |
| [Process](./Process.md) | **`Id`** `0`; **`Exit`** only succeeds for code `0`; **`Run`** errors |

Child types under **`System/Syscall/`**
(`StandardStream`, `Descriptor`, `ReadLimit`, `SyscallError`, `WriteRequest`, `ReadRequest`)
are re-exported from **`Prelude.bd`**.

Terminal styling and controls live in the **`corelib_console`** package (`Console`, `Ansi` modules), re-exported from the aggregator prelude.
