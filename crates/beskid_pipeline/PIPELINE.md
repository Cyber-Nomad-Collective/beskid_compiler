# Compilation pipeline phases

Stable phase ids live in `src/phases.rs`. The suggested order for a full `beskid build` is
`FULL_BUILD_PHASE_ORDER`.

## Front end and mods (platform-spec)

Normative string ids (Compiler Mods / stage ordering):

| Id | Constant | Typical emitter |
|----|----------|-----------------|
| `macro.expand` | `MACRO_EXPAND` | After `parse`, expand language `macro` rules before mod load |
| `mod.load` | `MOD_LOAD` | After `macro.expand`, when mod AOT artifacts and contract descriptors are available |
| `mod.collect` | `MOD_COLLECT` | Collector contracts declare generation targets |
| `mod.generate` | `MOD_GENERATE` | Generators emit typed AST contributions |
| `syntax.generation` | `SYNTAX_GENERATION` | After a `Program` snapshot exists (initial parse or re-parse) |
| `semantic` | `SEMANTIC` | Builtin semantic rules / diagnostics gate |
| `semantic.snapshot` | `SEMANTIC_SNAPSHOT` | Immediately after semantic rules complete for the generation |
| `composition.resolve` | `COMPOSITION_RESOLVE` | Resolve native host DI graph and emit composition snapshot |
| `mod.analyze` | `MOD_ANALYZE` | Analyzer contracts after semantic snapshot |
| `mod.rewrite` | `MOD_REWRITE` | Rewriter contracts after analysis |
| `lower.ready` | `LOWER_READY` | Instant boundary immediately before the `lower` entrypoint |
| `lower` | `LOWER` | HIR normalize, resolve, type, then codegen inputs |
| `workspace.graph_changed` | `WORKSPACE_GRAPH_CHANGED` | After a workspace compile graph is (re)built |

**Full build** — `FULL_BUILD_PHASE_ORDER` lists: resolve → `workspace.graph_changed` →
`workspace.materialize` → `program.assemble` → `parse` → `macro.expand` → `mod.load` → `mod.collect` → `mod.generate` →
`syntax.generation` → `semantic` → `semantic.snapshot` → `composition.resolve` → `mod.analyze` → `mod.rewrite` →
`lower.ready` → `lower` → `codegen_clif` → AOT tail as today.

**Mod rebuild** — `MOD_BUILD_PHASE_ORDER` lists: resolve → `workspace.materialize` → `program.assemble` → `parse` →
`lower.ready` → `lower` → `codegen_clif` → `aot.emit_object` → `aot.link` for Mod package AOT
artifacts only (no host `mod.*` orchestration or `aot.runtime`).

**JIT run** — `JIT_RUN_PHASE_ORDER` uses the same mod + syntax + semantic snapshot + `composition.resolve` + `mod.analyze` /
`mod.rewrite` + `lower.ready` prefix after `parse`, then `lower`, `codegen_clif`, `jit.emit`,
`jit.finalize`. Used by interim `beskid test` and `beskid repl` (JIT snippet eval).

**AOT run** — `RUN_AOT_PHASE_ORDER` shares the same mod-enabled front-end prefix as `JIT_RUN_PHASE_ORDER`,
then `lower`, `codegen_clif`, `aot.emit_object`, `aot.runtime`, and `aot.link`. Target path for
`beskid run` (subprocess execution after link).

Hosts **must** emit `lower.ready` even when no mods ran so observers see a uniform ordering before
`lower`.

`beskid test` still compiles with the in-process JIT instead of emitting an object and linking.
After front-end phases (`parse` … `codegen_clif`), the engine reports `jit.emit` work units (one per
lowered function) and a `jit.finalize` phase around Cranelift `finalize_definitions`. AOT steps
(`aot.emit_object`, `aot.runtime`, `aot.link`) are skipped on that path.

Future `beskid test` AOT migration is phase 2; interim test runs still use the JIT path above.

`beskid run` uses `RUN_AOT_PHASE_ORDER`: build a linked executable via `beskid_aot::build`
(or `build_and_run`) and execute it in a subprocess. JIT phases are not observed on that path.

Emitters (`beskid_analysis`, `beskid_codegen`, `beskid_aot`, `beskid_engine`) report `PipelineEvent`
values; the CLI implements `PipelineObserver` and maps them to terminal UX.

When adding a new phase, define a new `&'static str` constant and append it to the documentation
here and, if applicable, to `FULL_BUILD_PHASE_ORDER`. Do not rename existing ids once released.
