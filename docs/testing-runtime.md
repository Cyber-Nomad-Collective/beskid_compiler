# Runtime Test Strategy and Coverage Map

## Suites
- Unit/runtime behavior: `crates/beskid_tests/src/runtime/*`
- ABI snapshot contracts: `crates/beskid_tests/src/abi/contracts.rs`
- Interop + extern security: `crates/beskid_tests/src/interop/*`

## Coverage highlights
- Alloc/GC root helpers
- Strings (UTF-8, concat, null checks)
- Events semantics (subscribe/unsubscribe/overflow)
- Scheduler primitives (`sched` feature)
- Metrics counters (`metrics` feature)
- JIT/AOT parity spot checks

## CI matrix intent
- Linux: full runtime + ABI + interop + extern paths
- macOS/Windows: compile checks and runtime subsets when extern dlopen paths are unavailable
- Linux sanitizer lanes for runtime crates
