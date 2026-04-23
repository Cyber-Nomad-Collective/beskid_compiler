# Runtime FFI and Externs (v1.0)

## Allowed type policy
Current supported extern mapping for analysis and runtime wiring:
- Params: `bool`, `u8`, `i32`, `i64`, `f64`, pointer-sized integer transport (`usize`/`i64` boundary strategy)
- Returns: same set plus `unit`

## Platform strategy
- Linux: `extern_dlopen` path is primary.
- Other platforms: compile-time or runtime stubs should fail with explicit diagnostics.

## MVP corelib/runtime ABI touchpoints
For compile/run MVP with checked-in corelib (`corelib/beskid_corelib`), runtime exports must keep these symbols stable:
- `str_len` (used by `Core.String`)
- `sys_print` and `sys_println` (used by `System.IO`)

These are validated in `crates/beskid_tests/src/abi/contracts.rs`.

## Security controls
- `BESKID_EXTERN_ALLOW`
- `BESKID_EXTERN_DENY`

Pattern forms:
- `lib:symbol`
- `lib:*`
- `*:symbol`
- `symbol`

Evaluation rules:
- If allowlist is non-empty, symbol must match at least one allow pattern.
- Denylist is then applied and takes precedence when both match.
- Wildcard prefix/suffix patterns are supported (`*`), e.g. `libc.so.*:getpid`.

## Demos
See `compiler/examples/extern` and runtime/interop tests for `getpid`/`write`-style usage.
