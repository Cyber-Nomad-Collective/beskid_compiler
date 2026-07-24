//! Workspace resolution, parsing, semantic analysis helpers, and IDE-oriented
//! document queries (hover, definitions, completions).

mod analyze;
mod composition;
mod diagnostics_emit;
mod document;
#[cfg(test)]
mod document_tests;
mod entry_session;
mod front_end;
mod input;
mod lower;
mod parse;
mod parse_recovery;
mod prepare;
mod project;
mod unit_ops;

#[doc(hidden)]
#[deprecated(
    since = "0.5.0",
    note = "renamed to `unit_ops`; import helpers from `beskid_analysis::services` re-exports"
)]
pub mod queries {
    pub use super::unit_ops::*;
}
mod render;
mod semantic;
mod session;
#[cfg(test)]
mod session_tests;
mod synthetic_plan;
#[cfg(test)]
mod test_support;

#[allow(deprecated)]
pub use analyze::{
    analyze_file_in_project, analyze_program, analyze_program_with_options, analyze_source_in_project,
    analyze_source_in_project_with_options, analyze_source_with_compilation_context, compile_plan_for_input_path,
    compile_plan_for_input_path_with_member,
};
pub use composition::{
    composition_diagnostics_for_program, composition_result_to_diagnostics, prepare_program_for_composition,
    resolve_program_composition,
};
pub use diagnostics_emit::{parse_error_diagnostic, pest_error_diagnostic, project_error_diagnostic};
pub use document::{
    AnalysisSymbolKind, CompletionInfo, CompletionKind, DefinitionInfo, DocumentAnalysisSnapshot, DocumentSymbolInfo,
    HoverInfo, ReferenceInfo, SymbolLocation, TestCaseInfo, assemble_for_api_documentation,
    build_api_documentation_snapshot, build_document_analysis, build_document_analysis_for_resolved,
    build_document_analysis_from_resolution, build_document_analysis_with_context, collect_document_symbols,
    collect_test_cases, completion_candidates, definition_at_offset, hover_at_offset, item_id_at_offset,
    references_at_offset, references_at_offset_workspace, resolve_assembly_for_api_documentation, symbol_kind_name,
};
pub use entry_session::{
    composition_fingerprint, current_syntax_generation_id, get_or_insert_assembly,
    invalidate_all as invalidate_entry_sessions, invalidate_project as invalidate_entry_sessions_for_project,
    next_syntax_generation_id, update_semantic_snapshot,
};
pub use front_end::{FrontEndOptions, FrontEndTypedResult, compile_front_end_with_pipeline};
pub use input::{
    AnalyzeInProjectOptions, ResolvedInput, resolve_input, resolve_input_with_pipeline, resolve_input_with_policy,
};
pub use lower::{
    DependencyTypingPolicy, LowerResolveTypeError, TypedHirResolution, lower_normalize_resolve_type_spanned,
    lower_normalize_resolve_type_spanned_with_assembly, typed_hir_from_lowered,
};
pub use parse::{
    ParsedProgram, parse_expression_source, parse_program, parse_program_with_source_name,
    parse_program_with_source_name_and_diagnostics,
};
pub use prepare::{
    PrepareOptions, PreparedCompilation, prepare_compilation, prepare_compilation_diagnostics, resolved_input_from_plan,
};
pub use project::{ResolvedProject, resolve_project, resolve_project_with_policy};
pub use render::render_program_tree;
pub use semantic::{
    SemanticDiagnosticsError, require_no_semantic_errors, semantic_rule_diagnostics_for_program,
    semantic_rule_diagnostics_for_program_with_pipeline,
};
pub use session::{
    CompilationSession, SEMANTIC_SNAPSHOT_VERSION, SemanticSnapshot, SessionFingerprint, cached_compilation_session,
    cached_executable, cached_semantic_snapshot, session_for_assembly, store_executable_on_session,
};
pub use synthetic_plan::synthetic_compile_plan_for_source;
pub use unit_ops::{
    assemble_unit, invalidate_dependents, module_index_query, resolve_entry, type_dep_signatures, type_entry,
    type_entry_gate,
};
