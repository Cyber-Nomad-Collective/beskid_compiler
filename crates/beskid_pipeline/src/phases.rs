//! Stable phase identifiers for observers and UX. Add new constants; do not rename.
//!
//! Meta-related ids align with platform-spec `compiler/metaprogramming-mod-sdk` and
//! `compiler/build-pipeline/stage-ordering` (see crate `PIPELINE.md`).

/// Locate and select `Project.proj` / workspace member.
pub const RESOLVE_MANIFEST: &str = "resolve.manifest";
/// Build dependency graph / compile plan.
pub const RESOLVE_GRAPH: &str = "resolve.graph";
/// Workspace compile graph changed (refresh / incremental orchestration).
pub const WORKSPACE_GRAPH_CHANGED: &str = "workspace.graph_changed";
/// Lockfile + materialize dependency trees.
pub const WORKSPACE_MATERIALIZE: &str = "workspace.materialize";
/// Parse Beskid source.
pub const PARSE: &str = "parse";
/// Meta host attached after workspace binds `attachTo` / entry modules (host compilation).
pub const META_HOST_ATTACHED: &str = "meta.host_attached";
/// Start of one meta scheduling round (capture / process).
pub const META_ROUND_START: &str = "meta.round_start";
/// Immutable `Program` snapshot for a syntax generation (initial parse or re-parse after emit).
pub const SYNTAX_GENERATION: &str = "syntax.generation";
/// End of one meta round after atomic merge of emit contributions.
pub const META_ROUND_COMMIT: &str = "meta.round_commit";
/// Semantic rules / diagnostics gate.
pub const SEMANTIC: &str = "semantic";
/// Builtin semantic rules finished for the generation (snapshot boundary for inspectors).
pub const SEMANTIC_SNAPSHOT: &str = "semantic.snapshot";
/// Merged program ready for HIR lowering (instant boundary immediately before [`LOWER`]).
pub const LOWER_READY: &str = "lower.ready";
/// HIR lowering and middle-end.
pub const LOWER: &str = "lower";
/// CLIF generation from typed HIR.
pub const CODEGEN_CLIF: &str = "codegen_clif";
/// Cranelift object emission (per-function work units common here).
pub const AOT_EMIT_OBJECT: &str = "aot.emit_object";
/// JIT per-function define (mirrors `AOT_EMIT_OBJECT` work units for progress UX).
pub const JIT_EMIT: &str = "jit.emit";
/// JIT `finalize_definitions` boundary.
pub const JIT_FINALIZE: &str = "jit.finalize";
/// Runtime archive preparation / build-on-the-fly.
pub const AOT_RUNTIME: &str = "aot.runtime";
/// Platform linker invocation.
pub const AOT_LINK: &str = "aot.link";

/// Suggested coarse order for a full CLI `beskid build` (for documentation and tests).
///
/// After [`PARSE`], meta rounds use one illustrative
/// [`META_ROUND_START`] → [`SYNTAX_GENERATION`] → [`META_ROUND_COMMIT`] spine; hosts may emit
/// additional `syntax.generation` / round pairs per bounded emit loop.
pub const FULL_BUILD_PHASE_ORDER: &[&str] = &[
    RESOLVE_MANIFEST,
    RESOLVE_GRAPH,
    WORKSPACE_GRAPH_CHANGED,
    WORKSPACE_MATERIALIZE,
    PARSE,
    META_HOST_ATTACHED,
    META_ROUND_START,
    SYNTAX_GENERATION,
    META_ROUND_COMMIT,
    SEMANTIC,
    SEMANTIC_SNAPSHOT,
    LOWER_READY,
    LOWER,
    CODEGEN_CLIF,
    AOT_EMIT_OBJECT,
    AOT_RUNTIME,
    AOT_LINK,
];

/// Phases observed for a typical `beskid run` / `beskid test` JIT path after resolution
/// (no AOT runtime or link steps).
///
/// The `meta.*` / `syntax.generation` prefix reflects a **meta-enabled** host. Helpers that lower
/// parsed source without running meta begin reporting at [`PARSE`] and still emit [`LOWER_READY`]
/// before [`LOWER`].
pub const JIT_RUN_PHASE_ORDER: &[&str] = &[
    PARSE,
    META_HOST_ATTACHED,
    META_ROUND_START,
    SYNTAX_GENERATION,
    META_ROUND_COMMIT,
    SEMANTIC,
    SEMANTIC_SNAPSHOT,
    LOWER_READY,
    LOWER,
    CODEGEN_CLIF,
    JIT_EMIT,
    JIT_FINALIZE,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(order: &[&str], id: &'static str) -> Option<usize> {
        order.iter().position(|p| *p == id)
    }

    #[test]
    fn full_build_orders_meta_between_parse_and_semantic() {
        let o = FULL_BUILD_PHASE_ORDER;
        assert!(pos(o, PARSE).unwrap() < pos(o, META_HOST_ATTACHED).unwrap());
        assert!(pos(o, META_HOST_ATTACHED).unwrap() < pos(o, META_ROUND_START).unwrap());
        assert!(pos(o, META_ROUND_START).unwrap() < pos(o, SYNTAX_GENERATION).unwrap());
        assert!(pos(o, SYNTAX_GENERATION).unwrap() < pos(o, META_ROUND_COMMIT).unwrap());
        assert!(pos(o, META_ROUND_COMMIT).unwrap() < pos(o, SEMANTIC).unwrap());
        assert!(pos(o, SEMANTIC).unwrap() < pos(o, SEMANTIC_SNAPSHOT).unwrap());
        assert!(pos(o, SEMANTIC_SNAPSHOT).unwrap() < pos(o, LOWER_READY).unwrap());
        assert!(pos(o, LOWER_READY).unwrap() < pos(o, LOWER).unwrap());
        assert!(pos(o, RESOLVE_GRAPH).unwrap() < pos(o, WORKSPACE_GRAPH_CHANGED).unwrap());
        assert!(pos(o, WORKSPACE_GRAPH_CHANGED).unwrap() < pos(o, WORKSPACE_MATERIALIZE).unwrap());
    }

    #[test]
    fn jit_run_matches_meta_prefix_before_lower() {
        let o = JIT_RUN_PHASE_ORDER;
        assert!(pos(o, PARSE).unwrap() < pos(o, META_HOST_ATTACHED).unwrap());
        assert!(pos(o, META_ROUND_COMMIT).unwrap() < pos(o, SEMANTIC).unwrap());
        assert!(pos(o, SEMANTIC_SNAPSHOT).unwrap() < pos(o, LOWER_READY).unwrap());
        assert!(pos(o, LOWER_READY).unwrap() < pos(o, LOWER).unwrap());
        assert!(pos(o, LOWER).unwrap() < pos(o, CODEGEN_CLIF).unwrap());
    }
}
