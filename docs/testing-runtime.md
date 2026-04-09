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

## CI matrix (compiler repository)

CI for the toolchain lives in **this repository’s** `.github/workflows/ci.yml` (the compiler public remote — not an aggregate parent repo).

- **Linux (`ubuntu-latest`):** `cargo check --workspace`, full `cargo test -p beskid_tests`, explicit `abi::contracts::`, `cargo bench -p beskid_runtime --no-run`, full `cargo test -p beskid_e2e_tests` (after `cargo build -p beskid_cli`), `runtime::` tests under AddressSanitizer (nightly), `cargo test -p beskid_engine --features extern_dlopen security_allow_deny_sequences`.
- **macOS / Windows:** `cargo build -p beskid_cli` then `cargo test -p beskid_e2e_tests cli_cross_platform` (parse / tree / analyze smoke — no AOT link).
- **Release (`push` to `main` or tag `v*`):** `release-cli` uploads multi-target CLI artifacts after the jobs above succeed; requires `SEAWEEDFS_*` secrets.

Aggregate repos that only **submodule** this tree do not need a second copy of these jobs; run them on the compiler remote.

## CI matrix intent (runtime coverage)
- Linux: full runtime + ABI + interop + extern paths
- macOS/Windows: cross-platform CLI E2E smoke; full AOT/link remains Linux-first
- Linux sanitizer lanes for runtime crates

## Release gating
- See `docs/runtime-release-gates.md` for required runtime merge/release checks and PR evidence policy.

## Canonical stdlib source (MVP)
- Checked-in stdlib source of truth is `../corelib/standard_library` (corelib submodule), not ad-hoc copies.
- Compiler tests resolve only this canonical root from `crates/beskid_tests/src/projects/stdlib` and fail if missing.
- CLI provisioning embeds the same source tree and installs it to `BESKID_STDLIB_ROOT` (or `$HOME/.beskid/standard_library`).

## AOT limitations (linker)

Some AOT link modes return a structured error when **shared export policy flags** are not implemented for the active target (see `crates/beskid_aot/src/linker.rs`, message containing `shared export policy`). Treat that as an unsupported configuration for MVP unless the linker gains support for that target.

## AOT E2E tests
- Crate: `crates/beskid_e2e_tests`
- Scope: Linux-first black-box flow for `beskid` CLI `fetch/lock/update/build` and native executable run. Cross-platform jobs run only the `cli_cross_platform` module (no `nm` / native executable link).
- Fixtures: `crates/beskid_e2e_tests/fixtures/*`
- Included fixture categories:
  - `smoke_project`: minimal compile/link/run baseline.
  - `cross_platform_cli`: minimal source for parse/tree/analyze smoke on all OSes.
  - `analyze_diagnostics`: semantic error coverage for `beskid analyze`.
  - `smoke_project` build graph assertions: stdlib dependency auto-injection stays active.
  - `runtime_calls`: runtime builtins (`__str_len`, `__sys_println`) and lambda behavior.
  - `event_unsubscribe`: event subscribe/unsubscribe execution path.
  - `deps_workspace`: multi-project path-dependency materialization flow.
  - `contracts_dispatch`, `enums_match`, `method_dispatch`, `closure_capture`, `try_expression`: semantic E2E matrix projects.
  - `unresolved_registry`: unresolved external dependency failure contract.
  - `perf_loop`: compile+run performance smoke with configurable budgets.

### Local run
- Build CLI first so E2E tests can execute a real binary:
  - `cargo build -p beskid_cli`
- Run E2E suite:
  - `cargo test -p beskid_e2e_tests`
- Optional explicit binary path:
  - `BESKID_CLI_BIN=/abs/path/to/beskid_cli cargo test -p beskid_e2e_tests`

### Environment expectations
- Linux host with native linker toolchain (`cc`, `ar`, `ranlib`, `nm`) available in `PATH`.
- `CC` can override the linker used by AOT build path.
- Tests use isolated temporary directories and time-bounded process execution.
- Performance budget overrides:
  - `BESKID_E2E_BUILD_MAX_MS` (default `180000`)
  - `BESKID_E2E_RUN_MAX_MS` (default `30000`)
  - `BESKID_E2E_BATCH_BUILD_MAX_MS` (default `240000`)

### Expanded E2E suite notes
- Runtime-sensitive scenarios are validated via both:
  - `beskid run` for behavioral execution assertions.
  - `beskid build` for AOT artifact/link checks and symbol-level validation.
- Semantic matrix cases additionally execute compiled native binaries and assert expected exit codes.
