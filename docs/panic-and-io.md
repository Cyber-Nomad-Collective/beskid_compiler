# Runtime Panic/IO Diagnostics (v1.0)

## Panic policy
- Panic messages should be explicit and stable for common invariant failures:
  - `null string handle`
  - `null string data`
  - `invalid utf-8 string data`
  - `event capacity exceeded`
  - runtime scope violations

## IO builtins
- `sys_print` and `sys_println` require valid non-null string handles.
- Linux/x86_64 uses syscall path for stdout writes.
- Other targets use stdio fallback.

## CLI integration
CLI parse/semantic surfaces are expected to be miette-first.
Runtime panic wrapping may be layered at CLI boundary where needed without obscuring debug paths.
