# Runtime Panic / IO (v1.0)

## Panic policy

- Panic messages should be explicit and stable for common invariant failures:
  - `null string handle`
  - `null string data`
  - `invalid utf-8 string data`
  - `event capacity exceeded`
  - runtime scope violations

## IO boundary

- **`syscall_write`**: requires a valid non-null string handle. First argument is a **normalized file descriptor** (`1` = stdout and `2` = stderr on common targets; Linux supports additional fds via the `write` syscall; other hosts may return `-1` for unsupported fds).
- Linux/x86_64 uses a syscall path for writes; other targets use stdio for fds `1` and `2`.
- Legacy **`sys_print` / `sys_println`** have been removed; use **`syscall_write`** (via `System.Syscall.Write` in corelib) instead.

## CLI integration

CLI parse/semantic surfaces are expected to be miette-first.
Runtime panic wrapping may be layered at the CLI boundary where needed without obscuring debug paths.
