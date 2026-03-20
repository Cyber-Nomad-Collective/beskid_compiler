# Runtime FFI and Externs (v1.0)

## Allowed type policy
Current supported extern mapping for analysis and runtime wiring:
- Params: `bool`, `u8`, `i32`, `i64`, `f64`, pointer-sized integer transport (`usize`/`i64` boundary strategy)
- Returns: same set plus `unit`

## Platform strategy
- Linux: `extern_dlopen` path is primary.
- Other platforms: compile-time or runtime stubs should fail with explicit diagnostics.

## Security controls
- `BESKID_EXTERN_ALLOW`
- `BESKID_EXTERN_DENY`

Pattern forms:
- `lib:symbol`
- `lib:*`
- `*:symbol`
- `symbol`

Allowlist is applied before denylist checks.

## Demos
See `compiler/examples/extern` and runtime/interop tests for `getpid`/`write`-style usage.
