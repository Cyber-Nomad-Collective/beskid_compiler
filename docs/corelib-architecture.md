# Corelib Architecture Inventory

This document maps the current corelib integration points and the Phase 1 consolidation target.

## Source of truth

- Canonical corelib source: `compiler/corelib/beskid_corelib`.
- Manifest: `Project.proj`.
- Runtime packaging entry: `src/Prelude.bd`.

## Coupling map (before refactor)

- Path discovery:
  - `crates/beskid_analysis/src/projects/graph/resolver.rs`
  - `crates/beskid_cli/build.rs`
  - `crates/beskid_tests/src/projects/corelib/mod.rs`
- Provisioning / copy:
  - `crates/beskid_cli/build.rs` (embed copy)
  - `crates/beskid_cli/src/corelib_runtime.rs` (install copy)
- Manifest parsing:
  - `crates/beskid_cli/src/corelib_runtime.rs`

## Consolidation target

Compiler integration points should share the same policy:

- source candidate resolution (`beskid_corelib`),
- root discovery from repository ancestors,
- canonical `Project.proj` project identity and version parsing,
- shared env-var compatibility (`BESKID_CORELIB_*`).

## Compatibility policy (Phase 1)

- Keep legacy directory and env variables working.
- Enforce `beskid_corelib` naming in project identity and publish flow.
- Introduce shared logic first, then progressively tighten enforcement in CI/tests.
