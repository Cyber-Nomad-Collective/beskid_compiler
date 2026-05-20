//! Semantic analysis for Beskid: parse → HIR → resolve → types, project graphs, IDE helpers, and rules.

#![allow(
    clippy::collapsible_match,
    clippy::field_reassign_with_default,
    clippy::large_enum_variant,
    clippy::module_inception,
    clippy::question_mark,
    clippy::too_many_arguments,
    clippy::type_complexity
)]
//!
//! Public entry points include [`services`] (workspace-aware analysis), [`projects`] (manifests and
//! compile plans), and re-exports for queries, diagnostics, and formatting.

pub mod analysis;
pub mod builtins;
pub mod compilation_context;
#[doc(hidden)]
#[allow(dead_code)]
pub mod compiler_sdk_reflect;
pub mod doc;
pub mod doc_comment_parser;
pub mod format;
pub mod hir;
pub mod mod_host;
pub mod parser;
pub mod parsing;
pub mod projects;
pub mod query;
pub mod resolve;
pub mod services;
pub mod syntax;
pub mod types;

pub use analysis::{
    AnalysisOptions, AnalysisResult, MietteReportError, Rule as AnalysisRule, RuleContext,
    SemanticDiagnostic, Severity, builtin_rules, run_rules,
};
pub use compilation_context::{CompilationContext, module_roots_for_plan};
pub use parser::{BeskidParser, Rule};
pub use projects::{
    ProjectGraphBuildOptions, WorkspaceResolutionSummary, resolve_project_manifest_for_source_path,
};
pub use query::{
    AstNode, Descendants, DynNodeRef, HirDescendants, HirNode, HirNodeKind, HirNodeRef, HirQuery,
    HirVisit, HirWalker, NodeKind, Query,
};
pub use services::{
    AnalyzeInProjectOptions, analyze_program_with_options, analyze_source_with_compilation_context,
    compile_plan_for_input_path, compile_plan_for_input_path_with_member,
    resolve_input_with_pipeline,
};
