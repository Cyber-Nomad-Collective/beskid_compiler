//! Human-readable labels for stable pipeline phase ids.

use beskid_pipeline::phases;

/// Short title for a pipeline phase id shown in the CLI progress UI.
pub fn phase_label(id: &str) -> &str {
    match id {
        phases::RESOLVE_MANIFEST => "Resolve manifest",
        phases::RESOLVE_GRAPH => "Resolve dependency graph",
        phases::WORKSPACE_GRAPH_CHANGED => "Refresh workspace graph",
        phases::WORKSPACE_MATERIALIZE => "Materialize dependencies",
        phases::PARSE => "Parse sources",
        phases::MOD_LOAD => "Load compiler mods",
        phases::MOD_COLLECT => "Collect mod targets",
        phases::MOD_GENERATE => "Generate from mods",
        phases::SYNTAX_GENERATION => "Syntax generation",
        phases::SEMANTIC => "Semantic analysis",
        phases::SEMANTIC_SNAPSHOT => "Semantic snapshot",
        phases::MOD_ANALYZE => "Analyze with mods",
        phases::MOD_REWRITE => "Rewrite with mods",
        phases::LOWER_READY => "Prepare lowering",
        phases::LOWER => "Lower to HIR",
        phases::CODEGEN_CLIF => "Generate CLIF",
        phases::AOT_EMIT_OBJECT => "Emit object code",
        phases::JIT_EMIT => "JIT compile",
        phases::JIT_FINALIZE => "Finalize JIT module",
        phases::AOT_RUNTIME => "Load runtime library",
        phases::AOT_LINK => "Link native artifact",
        _ => id,
    }
}
