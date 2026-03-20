# Extern cookbook

Quick, copy‑pasteable extern patterns.

## Declare an extern contract (Linux)

[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 getpid();
}

## Call a function

pub i64 main() { return C.getpid(); }

## Gate security in dev

# only allow getpid, block others
BESKID_EXTERN_ALLOW=libc.so.6:getpid
# or deny a symbol
BESKID_EXTERN_DENY=libc.so.6:getpid

## Test locally

# Full engine tests
cargo test -p beskid_engine --features extern_dlopen

# Focused external call
cargo test -p beskid_engine extern_real_call_getpid --features extern_dlopen

## Notes

- Allowed FFI types: params (bool, u8, i32, i64, f64, and `ref u8`), return (bool, u8, i32, i64, f64, or unit)
- Disallowed: strings, arrays, named types, other refs
- Non‑Linux platforms currently skip dynamic extern tests

