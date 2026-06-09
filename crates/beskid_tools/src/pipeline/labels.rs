//! Human-readable labels for stable pipeline phase ids.

use beskid_pipeline::phases;

/// Semantic rule pipeline sub-phases under [`phases::SEMANTIC`].
pub const SEMANTIC_SUB_PHASE_ORDER: &[&str] = &[
    phases::SEMANTIC_AST_LOWER,
    phases::SEMANTIC_DEFINITIONS,
    phases::SEMANTIC_CONTROL_FLOW,
    phases::SEMANTIC_NAME_RESOLUTION,
    phases::SEMANTIC_VISIBILITY,
    phases::SEMANTIC_CONTRACTS,
    phases::SEMANTIC_ERROR_HANDLING,
    phases::SEMANTIC_NAMING_STYLE,
];

/// Lower spine inside [`phases::LOWER`] (AST lower through type check).
pub const LOWER_FULL_SUB_PHASE_ORDER: &[&str] = &[
    phases::LOWER_AST,
    phases::LOWER_RESOLVE_PASS1,
    phases::LOWER_NORMALIZE,
    phases::LOWER_RESOLVE,
    phases::LOWER_TYPE_CHECK,
];

/// Workspace materialization sub-phases under [`phases::WORKSPACE_MATERIALIZE`].
pub const MATERIALIZE_SUB_PHASE_ORDER: &[&str] = &[
    phases::WORKSPACE_MATERIALIZE_LOCAL,
    phases::WORKSPACE_MATERIALIZE_PATH_DEPS,
    phases::WORKSPACE_MATERIALIZE_REGISTRY,
    phases::WORKSPACE_MATERIALIZE_LOCKFILE,
];

/// Ordered sub-phases for a parent phase id (stage progress bar denominator).
pub fn sub_phases_for_parent(parent_id: &str) -> Option<&'static [&'static str]> {
    match parent_id {
        phases::SEMANTIC => Some(SEMANTIC_SUB_PHASE_ORDER),
        phases::LOWER => Some(LOWER_FULL_SUB_PHASE_ORDER),
        phases::WORKSPACE_MATERIALIZE => Some(MATERIALIZE_SUB_PHASE_ORDER),
        _ => None,
    }
}

/// Index of `child_id` within a parent's ordered sub-phases, if known.
pub fn sub_phase_index(parent_id: &str, child_id: &str) -> Option<(usize, usize)> {
    let order = sub_phases_for_parent(parent_id)?;
    let index = order.iter().position(|id| *id == child_id)?;
    Some((index, order.len()))
}

/// Short title for a pipeline phase id shown in the CLI progress UI.
pub fn phase_label(id: &str) -> &str {
    match id {
        phases::RESOLVE_MANIFEST => "Resolve manifest",
        phases::RESOLVE_GRAPH => "Resolve dependency graph",
        phases::WORKSPACE_GRAPH_CHANGED => "Refresh workspace graph",
        phases::WORKSPACE_MATERIALIZE => "Materialize dependencies",
        phases::WORKSPACE_MATERIALIZE_LOCAL => "Copy project sources",
        phases::WORKSPACE_MATERIALIZE_PATH_DEPS => "Copy path dependencies",
        phases::WORKSPACE_MATERIALIZE_REGISTRY => "Fetch registry packages",
        phases::WORKSPACE_MATERIALIZE_LOCKFILE => "Sync lockfile",
        phases::PROGRAM_ASSEMBLE => "Assemble program",
        phases::PARSE => "Parse sources",
        phases::MACRO_EXPAND => "Expand macros",
        phases::MOD_LOAD => "Load compiler mods",
        phases::MOD_COLLECT => "Collect mod targets",
        phases::MOD_GENERATE => "Generate from mods",
        phases::SYNTAX_GENERATION => "Syntax generation",
        phases::SEMANTIC => "Semantic analysis",
        phases::SEMANTIC_AST_LOWER => "Lower AST to HIR",
        phases::SEMANTIC_DEFINITIONS => "Collect definitions",
        phases::SEMANTIC_CONTROL_FLOW => "Check control flow",
        phases::SEMANTIC_NAME_RESOLUTION => "Resolve names",
        phases::SEMANTIC_VISIBILITY => "Check modules and visibility",
        phases::SEMANTIC_CONTRACTS => "Check contracts and methods",
        phases::SEMANTIC_ERROR_HANDLING => "Check error handling",
        phases::SEMANTIC_TYPE_CHECK => "Type check",
        phases::SEMANTIC_NAMING_STYLE => "Check naming style",
        phases::SEMANTIC_SNAPSHOT => "Semantic snapshot",
        phases::COMPOSITION_RESOLVE => "Resolve composition",
        phases::MOD_ANALYZE => "Analyze with mods",
        phases::MOD_REWRITE => "Rewrite with mods",
        phases::LOWER_READY => "Prepare lowering",
        phases::LOWER => "Lower to HIR",
        phases::LOWER_AST => "Lower AST to HIR",
        phases::LOWER_RESOLVE_PASS1 => "Resolve (pass 1)",
        phases::LOWER_NORMALIZE => "Normalize HIR",
        phases::LOWER_RESOLVE => "Resolve (pass 2)",
        phases::LOWER_TYPE_CHECK => "Type check",
        phases::CODEGEN_CLIF => "Generate CLIF",
        phases::AOT_EMIT_OBJECT => "Emit object code",
        phases::JIT_EMIT => "JIT compile",
        phases::JIT_FINALIZE => "Finalize JIT module",
        phases::AOT_RUNTIME => "Load runtime library",
        phases::AOT_LINK => "Link native artifact",
        _ => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_pipeline::phases;

    #[test]
    fn semantic_naming_style_is_last_semantic_sub_phase() {
        let (index, total) =
            sub_phase_index(phases::SEMANTIC, phases::SEMANTIC_NAMING_STYLE).expect("indexed");
        assert_eq!(index, 7);
        assert_eq!(total, 8);
    }

    #[test]
    fn lower_type_check_indexes_under_lower_parent() {
        let (index, total) =
            sub_phase_index(phases::LOWER, phases::LOWER_TYPE_CHECK).expect("indexed");
        assert_eq!(index, 4);
        assert_eq!(total, 5);
    }
}
