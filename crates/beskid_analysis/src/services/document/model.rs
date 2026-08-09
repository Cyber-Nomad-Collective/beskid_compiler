use std::collections::HashSet;
use std::path::PathBuf;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::doc::ResolvedDoc;
use crate::resolve::Resolution;
use crate::syntax::{Program, Spanned};

/// Legacy HIR-backed document snapshot retained for CLI `beskid doc` only.
///
/// LSP publish/refresh and prepare-spine consumers MUST use generation-bound syntax facts /
/// [`crate::projects::SyntaxProgramAssembly`] instead. Building this snapshot is not part of
/// the IDE authority path (CYB-65).
#[derive(Debug, Clone)]
pub struct DocumentAnalysisSnapshot {
    pub program: Spanned<Program>,
    pub resolution: Option<Resolution>,
    /// Same indexing as `resolution.items` when resolution is present; otherwise empty.
    pub item_docs: Vec<Option<ResolvedDoc>>,
    /// Documentation validation (codes `W161x`); empty when resolution failed.
    pub doc_diagnostics: Vec<SemanticDiagnostic>,
    /// Native IoC composition diagnostics (`E170x`); populated for IDE snapshot warm paths.
    pub composition_diagnostics: Vec<SemanticDiagnostic>,
    /// Entry file path used for IDE location fallbacks.
    pub source_path: PathBuf,
    /// Known logical module paths when assembly-backed resolution ran.
    pub assembly_module_paths: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    pub path: PathBuf,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisSymbolKind {
    Function,
    Test,
    Method,
    Type,
    Enum,
    Contract,
    Constant,
    Module,
    Use,
}

#[derive(Debug, Clone)]
pub struct DocumentSymbolInfo {
    pub name: String,
    pub kind: AnalysisSymbolKind,
    pub selection_start: usize,
    pub selection_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Method,
    Struct,
    Enum,
    Interface,
    Module,
    EnumMember,
    Variable,
    Text,
}

#[derive(Debug, Clone)]
pub struct CompletionInfo {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub markdown: String,
    pub location: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionInfo {
    pub location: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceInfo {
    pub location: SymbolLocation,
}

#[derive(Debug, Clone)]
pub struct TestCaseInfo {
    pub name: String,
    pub qualified_name: String,
    pub tags: Vec<String>,
    pub group: Option<String>,
    pub skip_condition: Option<bool>,
    pub skip_reason: Option<String>,
    pub selection_start: usize,
    pub selection_end: usize,
    /// 1-based line of the `test` name token (editor / terminal links).
    pub definition_line: usize,
    /// 1-based column of the `test` name token.
    pub definition_column: usize,
}
