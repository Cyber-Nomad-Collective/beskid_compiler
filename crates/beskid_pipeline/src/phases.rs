//! Stable phase identifiers for observers and UX. Add new constants; do not rename.
//!
//! Mod-related ids align with platform-spec `compiler/compiler-mods` and
//! `compiler/build-pipeline/stage-ordering` (see crate `PIPELINE.md`).

/// Locate and select `Project.proj` / workspace member.
pub const RESOLVE_MANIFEST: &str = "resolve.manifest";
/// Build dependency graph / compile plan.
pub const RESOLVE_GRAPH: &str = "resolve.graph";
/// Workspace compile graph changed (refresh / incremental orchestration).
pub const WORKSPACE_GRAPH_CHANGED: &str = "workspace.graph_changed";
/// Lockfile + materialize dependency trees.
pub const WORKSPACE_MATERIALIZE: &str = "workspace.materialize";
/// Copy the host project sources into the materialized workspace root.
pub const WORKSPACE_MATERIALIZE_LOCAL: &str = "workspace.materialize.local";
/// Copy path/workspace dependency project trees.
pub const WORKSPACE_MATERIALIZE_PATH_DEPS: &str = "workspace.materialize.path_deps";
/// Fetch and extract registry dependency packages.
pub const WORKSPACE_MATERIALIZE_REGISTRY: &str = "workspace.materialize.registry";
/// Write or validate the project lockfile after materialization.
pub const WORKSPACE_MATERIALIZE_LOCKFILE: &str = "workspace.materialize.lockfile";
/// Discover and parse compilation units from effective roots + compile plan.
pub const PROGRAM_ASSEMBLE: &str = "program.assemble";
/// Parse Beskid source.
pub const PARSE: &str = "parse";
/// Expand language `macro` rules (`name!` invocations) via typed AST substitution.
pub const MACRO_EXPAND: &str = "macro.expand";
/// Load mod AOT artifacts and contract descriptors for the active compile plan.
pub const MOD_LOAD: &str = "mod.load";
/// Run Collector contracts to declare generation targets.
pub const MOD_COLLECT: &str = "mod.collect";
/// Run Generator contracts and merge typed syntax contributions.
pub const MOD_GENERATE: &str = "mod.generate";
/// Immutable `Program` snapshot for a syntax generation (initial parse or re-parse after emit).
pub const SYNTAX_GENERATION: &str = "syntax.generation";
/// Semantic rules / diagnostics gate.
pub const SEMANTIC: &str = "semantic";
/// Lower parsed AST to HIR inside the semantic rule pipeline.
pub const SEMANTIC_AST_LOWER: &str = "semantic.ast_lower";
/// Collect and validate top-level definitions.
pub const SEMANTIC_DEFINITIONS: &str = "semantic.definitions";
/// Control-flow and pattern checks before name resolution.
pub const SEMANTIC_CONTROL_FLOW: &str = "semantic.control_flow";
/// Resolve names and build the symbol tables used by later stages.
pub const SEMANTIC_NAME_RESOLUTION: &str = "semantic.name_resolution";
/// Module graph and visibility rules.
pub const SEMANTIC_VISIBILITY: &str = "semantic.visibility";
/// Contract and method signature checks.
pub const SEMANTIC_CONTRACTS: &str = "semantic.contracts";
/// Error-handling (`try` / `catch`) rules.
pub const SEMANTIC_ERROR_HANDLING: &str = "semantic.error_handling";
/// Expression and declaration type checking.
pub const SEMANTIC_TYPE_CHECK: &str = "semantic.type_check";
/// Code style and naming convention checks (W163x).
pub const SEMANTIC_NAMING_STYLE: &str = "semantic.naming_style";
/// Builtin semantic rules finished for the generation (snapshot boundary for inspectors).
pub const SEMANTIC_SNAPSHOT: &str = "semantic.snapshot";
/// Native composition graph resolution for host DI (after semantic snapshot).
pub const COMPOSITION_RESOLVE: &str = "composition.resolve";
/// Run Analyzer contracts after semantic snapshot.
pub const MOD_ANALYZE: &str = "mod.analyze";
/// Run Rewriter contracts after analysis.
pub const MOD_REWRITE: &str = "mod.rewrite";
/// Run Beskid.Glue contracts (TypeMapping, SymbolEmission, LinkArgs,
/// SignatureReader, SignatureWriter, ToolchainProbe, StdioBridge) after
/// rewriter contracts and before lowering. Glue mods prepare foreign
/// bindings from the fully rewritten syntax.
pub const MOD_GLUE: &str = "mod.glue";
/// Merged program ready for HIR lowering (instant boundary immediately before [`LOWER`]).
pub const LOWER_READY: &str = "lower.ready";
/// HIR lowering and middle-end.
pub const LOWER: &str = "lower";
/// AST → HIR lowering inside the typed-HIR spine.
pub const LOWER_AST: &str = "lower.ast";
/// First resolution pass against assembly / module index.
pub const LOWER_RESOLVE_PASS1: &str = "lower.resolve_pass1";
/// HIR normalization after initial resolution.
pub const LOWER_NORMALIZE: &str = "lower.normalize";
/// Final resolution pass before type checking.
pub const LOWER_RESOLVE: &str = "lower.resolve";
/// Type-check the lowered program.
pub const LOWER_TYPE_CHECK: &str = "lower.type_check";
/// CLIF generation from typed HIR.
pub const CODEGEN_CLIF: &str = "codegen_clif";
/// Cranelift object emission (per-function work units common here).
pub const AOT_EMIT_OBJECT: &str = "aot.emit_object";
/// JIT per-function define (mirrors `AOT_EMIT_OBJECT` work units for progress UX).
pub const JIT_EMIT: &str = "jit.emit";
/// JIT `finalize_definitions` boundary.
pub const JIT_FINALIZE: &str = "jit.finalize";
/// Prebuilt runtime archive resolution and validation.
pub const AOT_RUNTIME: &str = "aot.runtime";
/// Platform linker invocation.
pub const AOT_LINK: &str = "aot.link";

/// Salsa query cache hit (incremental reuse).
pub const SALSA_QUERY_HIT: &str = "salsa.query_hit";
/// Salsa query cache miss (recomputed).
pub const SALSA_QUERY_MISS: &str = "salsa.query_miss";
/// Salsa input revision bump.
pub const SALSA_REVISION_BUMP: &str = "salsa.revision_bump";
/// On-disk unit artifact cache hit (`obj/beskid/cache/salsa/units/`).
pub const SALSA_ARTIFACT_DISK_HIT: &str = "salsa.artifact_disk_hit";
/// On-disk unit artifact cache miss (cold parse/HIR path).
pub const SALSA_ARTIFACT_DISK_MISS: &str = "salsa.artifact_disk_miss";

/// Suggested coarse order for a full CLI `beskid build` (for documentation and tests).
///
/// After [`PARSE`], mod phases run through [`MOD_REWRITE`] before [`LOWER_READY`]. Hosts may emit
/// additional [`SYNTAX_GENERATION`] boundaries when generator output triggers re-parse.
pub const FULL_BUILD_PHASE_ORDER: &[&str] = &[
    RESOLVE_MANIFEST,
    RESOLVE_GRAPH,
    WORKSPACE_GRAPH_CHANGED,
    WORKSPACE_MATERIALIZE,
    PROGRAM_ASSEMBLE,
    PARSE,
    MACRO_EXPAND,
    MOD_LOAD,
    MOD_COLLECT,
    MOD_GENERATE,
    SYNTAX_GENERATION,
    SEMANTIC,
    SEMANTIC_SNAPSHOT,
    COMPOSITION_RESOLVE,
    MOD_ANALYZE,
    MOD_REWRITE,
    MOD_GLUE,
    LOWER_READY,
    LOWER,
    CODEGEN_CLIF,
    AOT_EMIT_OBJECT,
    AOT_RUNTIME,
    AOT_LINK,
];

/// Phases observed for `beskid mod rebuild` / `beskid mod clean` prep (Mod package AOT only).
///
/// Resolve and materialize the workspace, compile the Mod project through object emission, then
/// link the mod artifact. Does not include host mod orchestration (`mod.load` … `mod.rewrite`) or
/// application runtime resolution.
pub const MOD_BUILD_PHASE_ORDER: &[&str] = &[
    RESOLVE_MANIFEST,
    RESOLVE_GRAPH,
    WORKSPACE_GRAPH_CHANGED,
    WORKSPACE_MATERIALIZE,
    PROGRAM_ASSEMBLE,
    PARSE,
    MACRO_EXPAND,
    LOWER_READY,
    LOWER,
    CODEGEN_CLIF,
    AOT_EMIT_OBJECT,
    AOT_LINK,
];

/// Phases observed for a typical `beskid run` / `beskid test` JIT path after resolution
/// (no AOT runtime or link steps).
///
/// The `mod.*` / `syntax.generation` prefix reflects a **mod-enabled** host. Helpers that lower
/// parsed source without running mods begin reporting at [`PARSE`] and still emit [`LOWER_READY`]
/// before [`LOWER`].
pub const JIT_RUN_PHASE_ORDER: &[&str] = &[
    PARSE,
    MACRO_EXPAND,
    MOD_LOAD,
    MOD_COLLECT,
    MOD_GENERATE,
    SYNTAX_GENERATION,
    SEMANTIC,
    SEMANTIC_SNAPSHOT,
    COMPOSITION_RESOLVE,
    MOD_ANALYZE,
    MOD_REWRITE,
    MOD_GLUE,
    LOWER_READY,
    LOWER,
    CODEGEN_CLIF,
    JIT_EMIT,
    JIT_FINALIZE,
];

/// Phases observed for `beskid run` after resolution when using the AOT subprocess path.
///
/// Same mod-enabled front-end prefix as [`JIT_RUN_PHASE_ORDER`], then object emission, runtime
/// archive resolution, and host link instead of in-process JIT finalize.
pub const RUN_AOT_PHASE_ORDER: &[&str] = &[
    PARSE,
    MACRO_EXPAND,
    MOD_LOAD,
    MOD_COLLECT,
    MOD_GENERATE,
    SYNTAX_GENERATION,
    SEMANTIC,
    SEMANTIC_SNAPSHOT,
    COMPOSITION_RESOLVE,
    MOD_ANALYZE,
    MOD_REWRITE,
    MOD_GLUE,
    LOWER_READY,
    LOWER,
    CODEGEN_CLIF,
    AOT_EMIT_OBJECT,
    AOT_RUNTIME,
    AOT_LINK,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(order: &[&str], id: &'static str) -> Option<usize> {
        order.iter().position(|p| *p == id)
    }

    #[test]
    fn full_build_orders_mod_between_parse_and_semantic() {
        let o = FULL_BUILD_PHASE_ORDER;
        assert!(pos(o, PARSE).unwrap() < pos(o, MACRO_EXPAND).unwrap());
        assert!(pos(o, MACRO_EXPAND).unwrap() < pos(o, MOD_LOAD).unwrap());
        assert!(pos(o, MOD_LOAD).unwrap() < pos(o, MOD_COLLECT).unwrap());
        assert!(pos(o, MOD_COLLECT).unwrap() < pos(o, MOD_GENERATE).unwrap());
        assert!(pos(o, MOD_GENERATE).unwrap() < pos(o, SYNTAX_GENERATION).unwrap());
        assert!(pos(o, SYNTAX_GENERATION).unwrap() < pos(o, SEMANTIC).unwrap());
        assert!(pos(o, SEMANTIC).unwrap() < pos(o, SEMANTIC_SNAPSHOT).unwrap());
        assert!(pos(o, SEMANTIC_SNAPSHOT).unwrap() < pos(o, MOD_ANALYZE).unwrap());
        assert!(pos(o, MOD_ANALYZE).unwrap() < pos(o, MOD_REWRITE).unwrap());
        assert!(pos(o, MOD_REWRITE).unwrap() < pos(o, MOD_GLUE).unwrap());
        assert!(pos(o, MOD_GLUE).unwrap() < pos(o, LOWER_READY).unwrap());
        assert!(pos(o, LOWER_READY).unwrap() < pos(o, LOWER).unwrap());
        assert!(pos(o, RESOLVE_GRAPH).unwrap() < pos(o, WORKSPACE_GRAPH_CHANGED).unwrap());
        assert!(pos(o, WORKSPACE_GRAPH_CHANGED).unwrap() < pos(o, WORKSPACE_MATERIALIZE).unwrap());
    }

    #[test]
    fn mod_build_orders_resolve_through_link_without_mod_orchestration() {
        let o = MOD_BUILD_PHASE_ORDER;
        assert!(pos(o, RESOLVE_MANIFEST).unwrap() < pos(o, WORKSPACE_MATERIALIZE).unwrap());
        assert!(pos(o, WORKSPACE_MATERIALIZE).unwrap() < pos(o, PROGRAM_ASSEMBLE).unwrap());
        assert!(pos(o, PROGRAM_ASSEMBLE).unwrap() < pos(o, PARSE).unwrap());
        assert!(pos(o, PARSE).unwrap() < pos(o, LOWER_READY).unwrap());
        assert!(pos(o, LOWER).unwrap() < pos(o, AOT_EMIT_OBJECT).unwrap());
        assert!(pos(o, AOT_EMIT_OBJECT).unwrap() < pos(o, AOT_LINK).unwrap());
        assert!(pos(o, MOD_LOAD).is_none());
        assert!(pos(o, SEMANTIC).is_none());
    }

    #[test]
    fn jit_run_matches_mod_prefix_before_lower() {
        let o = JIT_RUN_PHASE_ORDER;
        assert!(pos(o, PARSE).unwrap() < pos(o, MACRO_EXPAND).unwrap());
        assert!(pos(o, MACRO_EXPAND).unwrap() < pos(o, MOD_LOAD).unwrap());
        assert!(pos(o, MOD_GENERATE).unwrap() < pos(o, SEMANTIC).unwrap());
        assert!(pos(o, SEMANTIC_SNAPSHOT).unwrap() < pos(o, MOD_ANALYZE).unwrap());
        assert!(pos(o, MOD_ANALYZE).unwrap() < pos(o, MOD_REWRITE).unwrap());
        assert!(pos(o, MOD_REWRITE).unwrap() < pos(o, MOD_GLUE).unwrap());
        assert!(pos(o, MOD_GLUE).unwrap() < pos(o, LOWER_READY).unwrap());
        assert!(pos(o, LOWER_READY).unwrap() < pos(o, LOWER).unwrap());
        assert!(pos(o, LOWER).unwrap() < pos(o, CODEGEN_CLIF).unwrap());
    }

    #[test]
    fn run_aot_orders_aot_tail_after_codegen_clif() {
        let o = RUN_AOT_PHASE_ORDER;
        assert!(pos(o, PARSE).unwrap() < pos(o, MOD_LOAD).unwrap());
        assert!(pos(o, MOD_REWRITE).unwrap() < pos(o, MOD_GLUE).unwrap());
        assert!(pos(o, MOD_GLUE).unwrap() < pos(o, LOWER_READY).unwrap());
        assert!(pos(o, LOWER).unwrap() < pos(o, CODEGEN_CLIF).unwrap());
        assert!(pos(o, CODEGEN_CLIF).unwrap() < pos(o, AOT_EMIT_OBJECT).unwrap());
        assert!(pos(o, AOT_EMIT_OBJECT).unwrap() < pos(o, AOT_RUNTIME).unwrap());
        assert!(pos(o, AOT_RUNTIME).unwrap() < pos(o, AOT_LINK).unwrap());
        assert!(pos(o, JIT_EMIT).is_none());
        assert!(pos(o, JIT_FINALIZE).is_none());
    }
}
