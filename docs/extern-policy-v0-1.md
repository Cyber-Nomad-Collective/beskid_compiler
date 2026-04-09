# Extern policy v0.1

This document defines the initial extern calling policy and syntax for Beskid.

## Syntax

- Declare an extern contract with an Extern attribute:
  [Extern(Abi:"C", Library:"libc.so.6")]
  pub contract C {
      i64 getpid();
      // Example of a pointer parameter: write(1, buf, len)
      // i64 write(i32 fd, ref u8 buf, i64 len);
  }

- Call via language-level namespace:
  C.getpid()

- Dot vs :: semantics:
  - `mod::name` uses `::` for module paths
  - `C.getpid` uses `.` for namespaced member on a contract type (no instance)

## Validation (frontend)

- Enforced at type-check time:
  - Abi must be exactly "C" (case-insensitive)
  - Library must be provided and non-empty
  - Allowed FFI types:
    - params: bool, u8, i32, i64, f64, and pointer as `ref u8`
    - return: bool, u8, i32, i64, f64, or unit
  - Disallowed: strings, arrays, named types, functions, refs other than `ref u8`

Diagnostics (representative):
- ExternInvalidAbi
- ExternMissingLibrary
- ExternDisallowedParamType / ExternDisallowedReturnType

## Codegen and runtime

- Externs are imported dynamically on Linux via dlopen/dlsym behind feature `extern_dlopen`
- Calls to extern contract members like `C.getpid(...)` are lowered to direct external calls
- Contract-instance dispatch semantics are unchanged for non-extern contracts

## Security controls

- Optional environment (or test override) policies:
  - BESKID_EXTERN_ALLOW: comma-separated patterns
  - BESKID_EXTERN_DENY: comma-separated patterns
  - Pattern forms: `lib:symbol`, `lib:*`, `*:symbol`, or `symbol`; `*` is wildcard
- If allowlist is present, only matched symbols are allowed; denylist always blocks

## Examples

- getpid:
  [Extern(Abi:"C", Library:"libc.so.6")]
  pub contract C { i64 getpid(); }
  pub i64 main() { return C.getpid(); }

- write (advanced):
  [Extern(Abi:"C", Library:"libc.so.6")]
  // For now, pass pointers as i64 (pointer-sized). A future revision will support `ref u8` directly.
  pub contract C { i64 write(i32, i64, i64); }
  // Use the test helpers to get a pointer/len pair:
  //   __test_bytes_ptr() -> i64, __test_bytes_len() -> i64
  // See: crates/beskid_engine/tests/extern_write_demo.rs

