# Runtime Release Gates

Use this checklist for PRs that modify `beskid_runtime`, runtime ABI, or runtime-linked lowering paths.

## Required checks
- Runtime tests:
  - `cargo test -p beskid_tests runtime::`
- ABI contract checks:
  - `cargo test -p beskid_tests abi::contracts::`
- Stdlib MVP checks:
  - `cargo test -p beskid_tests projects::stdlib::`
  - `cargo test -p beskid_e2e_tests runtime_cases::`
  - `cargo build -p beskid_cli && cargo test -p beskid_e2e_tests cli_cross_platform::analyze_reports_no_diagnostics_for_minimal_file`
- Runtime benchmark compile gate:
  - `cargo bench -p beskid_runtime --no-run`

## Required evidence in PR description
- ABI impact statement:
  - `No ABI changes` **or** explicit list of symbol/signature changes.
- Runtime panic contract impact:
  - mention changed panic text if any and updated tests/docs.
- Performance impact:
  - note benchmark deltas or explain why not applicable.

## Mandatory follow-ups for ABI changes
- Update `crates/beskid_abi/src/symbols.rs` snapshot expectations.
- Update `docs/runtime-abi-v1.0.md`.
- Bump runtime ABI version when breaking.

## Optional deep validation
- Linux extern security tests (also run in `.github/workflows/ci.yml`):
  - `cargo test -p beskid_engine --features extern_dlopen security_allow_deny_sequences`
- Full CI dry-run (local equivalent): see `docs/testing-runtime.md` for the current matrix (workspace check, `beskid_tests`, e2e, ASan, etc.).
