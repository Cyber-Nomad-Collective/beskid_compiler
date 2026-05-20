//! Workspace resolution, parsing, semantic analysis helpers, and IDE-oriented
//! document queries (hover, definitions, completions).

mod analyze;
mod diagnostics_emit;
mod document;
mod front_end;
mod input;
mod lower;
mod parse;
mod project;
mod render;
mod semantic;

pub use analyze::{
    analyze_file_in_project, analyze_program, analyze_program_with_options,
    analyze_source_in_project, analyze_source_in_project_with_options,
    analyze_source_with_compilation_context, compile_plan_for_input_path,
    compile_plan_for_input_path_with_member,
};
pub use diagnostics_emit::{
    parse_error_diagnostic, pest_error_diagnostic, project_error_diagnostic,
};
pub use document::{
    AnalysisSymbolKind, CompletionInfo, CompletionKind, DefinitionInfo, DocumentAnalysisSnapshot,
    DocumentSymbolInfo, HoverInfo, ReferenceInfo, TestCaseInfo, build_document_analysis,
    collect_document_symbols, collect_test_cases, completion_candidates, definition_at_offset,
    hover_at_offset, references_at_offset, symbol_kind_name,
};
pub use front_end::{
    FrontEndOptions, FrontEndTypedResult, compile_front_end_from_resolved_input,
    compile_front_end_with_pipeline,
};
pub use input::{
    AnalyzeInProjectOptions, ResolvedInput, resolve_input, resolve_input_with_pipeline,
    resolve_input_with_policy,
};
pub use lower::{
    LowerResolveTypeError, lower_normalize_resolve_type_spanned,
    lower_normalize_resolve_type_spanned_with_assembly,
    lower_normalize_resolve_type_spanned_with_index,
};
pub use parse::{parse_program, parse_program_with_source_name};
pub use project::{ResolvedProject, resolve_project, resolve_project_with_policy};
pub use render::render_program_tree;
pub use semantic::{
    SemanticDiagnosticsError, require_no_semantic_errors, semantic_rule_diagnostics_for_program,
};
