# Compilation pipeline phases

Stable phase ids live in `src/phases.rs`. The suggested order for a full `beskid build` is
`FULL_BUILD_PHASE_ORDER`.

## Front end and meta (platform-spec)

Normative string ids (Metaprogramming Mod SDK / stage ordering):

| Id | Constant | Typical emitter |
|----|----------|-----------------|
| `meta.host_attached` | `META_HOST_ATTACHED` | After `parse`, when the host compilation knows attach / entry binding |
| `meta.round_start` | `META_ROUND_START` | Start of one scheduling round |
| `syntax.generation` | `SYNTAX_GENERATION` | After a `Program` snapshot exists (initial parse or re-parse) |
| `meta.round_commit` | `META_ROUND_COMMIT` | After atomic merge for that round |
| `semantic` | `SEMANTIC` | Builtin semantic rules / diagnostics gate |
| `semantic.snapshot` | `SEMANTIC_SNAPSHOT` | Immediately after semantic rules complete for the generation |
| `lower.ready` | `LOWER_READY` | Instant boundary immediately before the `lower` entrypoint |
| `lower` | `LOWER` | HIR normalize, resolve, type, then codegen inputs |
| `workspace.graph_changed` | `WORKSPACE_GRAPH_CHANGED` | After a workspace compile graph is (re)built |

**Full build** — `FULL_BUILD_PHASE_ORDER` lists: resolve → `workspace.graph_changed` →
`workspace.materialize` → `parse` → `meta.host_attached` → one illustrative
`meta.round_start` → `syntax.generation` → `meta.round_commit` → `semantic` → `semantic.snapshot` →
`lower.ready` → `lower` → `codegen_clif` → AOT/JIT tail as today.

**JIT run** — `JIT_RUN_PHASE_ORDER` uses the same meta + syntax + semantic snapshot + `lower.ready`
prefix after `parse`, then `lower`, `codegen_clif`, `jit.emit`, `jit.finalize`.

Hosts **must** emit `lower.ready` even when no meta ran so observers see a uniform ordering before
`lower`.

`beskid run` and `beskid test` compile with the in-process JIT instead of emitting an object and
linking. After front-end phases (`parse` … `codegen_clif`), the engine reports `jit.emit` work units
(one per lowered function) and a `jit.finalize` phase around Cranelift `finalize_definitions`. AOT
steps (`aot.emit_object`, `aot.runtime`, `aot.link`) are skipped on that path.

Emitters (`beskid_analysis`, `beskid_codegen`, `beskid_aot`, `beskid_engine`) report `PipelineEvent`
values; the CLI implements `PipelineObserver` and maps them to terminal UX.

When adding a new phase, define a new `&'static str` constant and append it to the documentation
here and, if applicable, to `FULL_BUILD_PHASE_ORDER`. Do not rename existing ids once released.
