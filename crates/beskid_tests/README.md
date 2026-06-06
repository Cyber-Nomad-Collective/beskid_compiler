# beskid_tests

Integration tests for the Beskid compiler workspace (`cargo test -p beskid_tests`).

## Layer taxonomy

| Layer | Module | What it covers |
|-------|--------|----------------|
| Surface | `surface/` | Pest rules plus AST shape (merged parsing + syntax) |
| Analysis | `analysis/` | Resolve, types, legality, lowering, pipeline rules |
| Codegen | `codegen/` | HIR lowering, descriptors, dynamic types |
| Runtime | `runtime/` | JIT execution, JIT vs AOT parity |
| Projects | `projects/` | Manifests, corelib layout, compile plans |
| Spine | `spine/` | Prepare → front-end → link-plan → lower conformance |
| Support | `support/` | Shared pipeline (`parse` → `resolve` → `typecheck`) and JIT helpers |

Add **pest-only** tests for malformed input and keyword rejection. Add **integration** tests when behavior spans resolve, types, or codegen.

## Shared harness

- `support::pipeline` — `parse_program`, `resolve`, `typecheck`, `typecheck_hir`, `lower_resolve`
- `support::runtime` — `compile_jit`, `jit_run_main_i64`, `compile_artifact`
- `projects::with_cwd` — mutex-guarded `set_current_dir` for project/corelib discovery

## Slow tests

Corelib-backed link completeness and `beskid_codegen` array link smoke are gated behind the `slow` feature:

```bash
cargo test -p beskid_tests --features slow
cargo test -p beskid_codegen --features slow
```

Default CI runs the fast tier only.
