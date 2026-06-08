# beskid_tests

Integration tests for the Beskid compiler workspace (`cargo test -p beskid_tests`).

## Layer taxonomy

| Layer | Module | What it covers |
|-------|--------|----------------|
| Surface | `surface/` | Pest rules plus AST shape (merged parsing + syntax) |
| Analysis | `analysis/` | Resolve, types, legality, lowering, pipeline rules |
| Codegen | `codegen/` | HIR lowering, descriptors, dynamic types |
| Runtime | `runtime/` | AOT execution, runtime API checks, AOT symbol parity |
| Projects | `projects/` | Manifests, corelib layout, compile plans |
| Spine | `spine/` | Prepare → front-end → link-plan → lower conformance |
| Support | `support/` | Shared pipeline (`parse` → `resolve` → `typecheck`) and AOT helpers |

Add **pest-only** tests for malformed input and keyword rejection. Add **integration** tests when behavior spans resolve, types, or codegen.

## Shared harness

- `support::pipeline` — `parse_program`, `resolve`, `typecheck`, `typecheck_hir`, `lower_resolve`
- `support::runtime` — `compile_artifact`, `aot_run_main_i64`, `build_aot_exe`, `with_runtime_scope`
- `projects::with_cwd` — mutex-guarded `set_current_dir` for project/corelib discovery

## Slow tests

Corelib-backed link completeness and `beskid_codegen` array link smoke are gated behind the `slow` feature:

```bash
cargo test -p beskid_tests --features slow
cargo test -p beskid_codegen --features slow
```

Default CI runs the fast tier only.

## Corelib spine gates (`spine::corelib_tests_*`)

The primary CI gate is a **single-process matrix** that typechecks every `corelib_tests`
entry with the semantic gate (`PrepareMode::DiagnosticsOnly`), not full executable lowering.

| Command | When |
| --- | --- |
| `cargo test -p beskid_tests corelib_tests_front_end_typechecks_matrix -- --nocapture --test-threads=1` | Full gate (~5–15 min debug, warm cache) |
| `BESKID_CORELIB_SPINE_SMOKE=1 cargo test … matrix …` | Fast local smoke (5 entries) |
| `BESKID_SKIP_CORELIB_SPINE=1 cargo test -p beskid_tests` | Skip spine while iterating elsewhere |
| `BESKID_CORELIB_SPINE_ENTRIES=text/TextCursorTests.bd cargo test … matrix …` | One entry via matrix driver |
| `cargo test -p beskid_tests text_cursor_tests_front_end_typechecks -- --ignored --test-threads=1` | Bisect one ignored per-entry helper |

**Hang prevention:** always pass `--test-threads=1` for spine work (process-global cwd/env locks).
Per-entry timeouts panic with remediation hints (`BESKID_SKIP_CORELIB_SPINE`, filter env vars).

**Do not** use a 90s wall-clock timeout on spine gates — assembly alone can take ~5s and a cold
semantic gate ~60s per entry. CI should allow **≥45 min** for full `cargo test --workspace` on
debug builds, or run `beskid_tests` with a **≥20 min** job timeout when scoped.
