//! Semantic analysis for Beskid: parse → HIR → resolve → types, project graphs, IDE helpers, and rules.

#![allow(
    clippy::collapsible_match,
    clippy::field_reassign_with_default,
    clippy::large_enum_variant,
    clippy::module_inception,
    clippy::too_many_arguments,
    clippy::type_complexity
)]
//!
//! Public entry points include [`services`] (workspace-aware analysis), [`projects`] (manifests and
//! compile plans), and re-exports for syntax queries, diagnostics, and formatting.

pub mod analysis;
pub mod artifacts;
pub mod builtins;
pub mod compilation_context;
#[doc(hidden)]
pub mod compiler_sdk_reflect;
pub mod composition;
pub mod doc;
pub mod doc_comment_parser;
pub mod external_library;
pub mod format;
pub mod hir;
pub mod macros;
pub mod mod_host;
pub mod naming_case;
pub(crate) mod naming_program;
pub mod parser;
pub mod parsing;
pub mod paths;
pub mod projects;
pub mod syntax_query;

#[doc(hidden)]
#[deprecated(
    since = "0.5.0",
    note = "renamed to `syntax_query`; use `beskid_analysis::syntax_query` instead"
)]
pub mod query {
    pub use super::syntax_query::*;
}
pub mod resolve;
pub mod runtime_registration;
pub mod services;
pub mod syntax;
pub mod types;

pub use analysis::{
    AnalysisOptions, AnalysisResult, MietteReportError, Rule as AnalysisRule, RuleContext,
    SemanticDiagnostic, Severity, builtin_rules, run_rules,
};
pub use compilation_context::{CompilationContext, ProjectSessionHandle, module_roots_for_plan};
pub use parser::{BeskidParser, Rule};
pub use projects::{AssemblyDiscovery, AssemblyOptions, ProgramAssembly, effective_roots_for_plan};
pub use projects::{
    ProjectGraphBuildOptions, WorkspaceResolutionSummary, resolve_project_manifest_for_source_path,
};
#[allow(deprecated)]
pub use services::{
    AnalyzeInProjectOptions, analyze_program_with_options, analyze_source_with_compilation_context,
    compile_plan_for_input_path, compile_plan_for_input_path_with_member,
    resolve_input_with_pipeline,
};
pub use syntax::{AstNodeId, AstNodeKey, SyntaxGenerationId};
pub use syntax_query::{
    Ancestors, AstNode, Descendants, DynNodeRef, HirDescendants, HirNode, HirNodeKind, HirNodeRef,
    HirQuery, HirVisit, HirWalker, NodeKind, Query, SyntaxNodeId, SyntaxSnapshot,
};
