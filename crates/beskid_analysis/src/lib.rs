//! Semantic analysis for Beskid: parse → syntax → resolve → types, project graphs, IDE helpers, and rules.

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

pub mod macros;
pub mod mod_host;
pub mod naming_case;
pub(crate) mod naming_program;
pub mod parser;
pub mod parsing;
pub mod paths;
pub mod projects;
pub mod syntax_query;

/// Mod-origin quick-fix shapes returned by the prepare spine and consumed by LSP
/// code actions. Re-exported here so `beskid_lsp` can name them as
/// `beskid_analysis::SyntaxFix` (single implementation — no LSP-side duplicate).
pub use mod_host::{SyntaxFix, SyntaxTextEdit, SyntaxTextEditKind};

pub mod resolve;
pub mod services;
pub mod syntax;
pub mod types;

pub use analysis::{
    AnalysisOptions, AnalysisResult, MietteReportError, Rule as AnalysisRule, RuleContext, SemanticDiagnostic,
    Severity, builtin_rules, run_rules,
};
pub use compilation_context::{CompilationContext, ProjectSessionHandle, module_roots_for_plan};
pub use parser::{BeskidParser, Rule};
pub use projects::{AssemblyDiscovery, AssemblyOptions, ProgramAssembly, effective_roots_for_plan};
pub use projects::{ProjectGraphBuildOptions, WorkspaceResolutionSummary, resolve_project_manifest_for_source_path};
#[allow(deprecated)]
pub use services::{
    AnalyzeInProjectOptions, analyze_program_with_options, analyze_source_with_compilation_context,
    compile_plan_for_input_path, compile_plan_for_input_path_with_member, resolve_input_with_pipeline,
};
pub use syntax::{AstNodeId, AstNodeKey, SyntaxGenerationId};
pub use syntax_query::{
    Ancestors, AstNode, Descendants, DynNodeRef, NodeKind, NodeRef, Query, SyntaxIndex, SyntaxNodeId, SyntaxSnapshot,
    Visit,
};
