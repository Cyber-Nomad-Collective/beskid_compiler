use std::borrow::ToOwned;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::compilation_context::ProjectSessionHandle;
use crate::doc::DocRefLinkContext;
use crate::doc::ResolvedDoc;
use crate::projects::assembly::{AssemblyError, ProgramAssembly};
use crate::projects::{
    AssemblyDiscovery, CompilePlan, PreparedProjectWorkspace, assemble_program,
    assembly_options_for_prepare,
};
use crate::resolve::{
    ItemId, ItemKind, LocalId, Resolution, ResolvedValue, SymbolId, canonical_item_id,
    symbol_for_item,
};
use crate::syntax::{Expression, Literal, Node, Program, Spanned, TestDefinition};

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

/// Assemble the entry import closure for `api.json` (same discovery as prepare / `beskid build`).
pub fn assemble_for_api_documentation(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
) -> Result<ProgramAssembly, AssemblyError> {
    let mut options = assembly_options_for_prepare(plan, AssemblyDiscovery::ImportClosure);
    options.skip_parse_errors = true;
    assemble_program(plan, workspace, entry_path, entry_source, &options, None)
}

/// Build a documentation snapshot from prepare-spine entry resolution and assembled units.
pub fn build_api_documentation_snapshot(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    resolution: Resolution,
    assembly: &ProgramAssembly,
    compile_plan: &CompilePlan,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let module_paths = assembly.module_index.known_module_path_strings();
    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        Some(resolution),
        module_paths,
        Some(assembly),
        Some(compile_plan),
        docs_ref_links,
    )
}

/// Full-project resolution for `api.json`: prefetch symbols from every unit, resolve entry, then merge type/value tables from each unit.
pub fn resolve_assembly_for_api_documentation(
    assembly: &ProgramAssembly,
    _entry_path: &Path,
) -> Option<Resolution> {
    assembly
        .module_index
        .resolve_for_api_documentation(assembly.entry_hir(), assembly)
}

fn symbol_location_for_item(
    item: &crate::resolve::ItemInfo,
    fallback_path: &Path,
) -> SymbolLocation {
    SymbolLocation {
        path: item
            .source_path
            .clone()
            .unwrap_or_else(|| fallback_path.to_path_buf()),
        start: item.span.start,
        end: item.span.end,
    }
}

fn symbol_location_for_span(path: &Path, start: usize, end: usize) -> SymbolLocation {
    SymbolLocation {
        path: path.to_path_buf(),
        start,
        end,
    }
}

fn resolved_value_at_offset(resolution: &Resolution, offset: usize) -> Option<&ResolvedValue> {
    resolution
        .tables
        .resolved_values
        .iter()
        .filter(|(span, _)| span.start <= offset && offset <= span.end)
        .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
        .map(|(_, resolved)| resolved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceTarget {
    Local(LocalId),
    Symbol(SymbolId),
    Item(ItemId),
}

fn reference_target(resolution: &Resolution, resolved: &ResolvedValue) -> ReferenceTarget {
    match resolved {
        ResolvedValue::Local(local_id) => ReferenceTarget::Local(*local_id),
        ResolvedValue::Item(item_id) => {
            if let Some(symbol) = symbol_for_item(resolution, *item_id) {
                ReferenceTarget::Symbol(symbol)
            } else {
                ReferenceTarget::Item(*item_id)
            }
        }
    }
}

fn reference_targets_match(
    entry_resolution: &Resolution,
    target: ReferenceTarget,
    unit_resolution: &Resolution,
    candidate: &ResolvedValue,
) -> bool {
    match (target, candidate) {
        (ReferenceTarget::Local(target_local), ResolvedValue::Local(candidate_local)) => {
            target_local == *candidate_local
        }
        (ReferenceTarget::Symbol(target_symbol), ResolvedValue::Item(candidate_item)) => {
            symbol_for_item(unit_resolution, *candidate_item) == Some(target_symbol)
        }
        (ReferenceTarget::Item(target_item), ResolvedValue::Item(candidate_item)) => {
            target_item == *candidate_item
        }
        (ReferenceTarget::Symbol(target_symbol), _) => {
            reference_target(entry_resolution, candidate) == ReferenceTarget::Symbol(target_symbol)
        }
        _ => false,
    }
}

fn analysis_symbol_kind_from_item_kind(kind: ItemKind) -> Option<AnalysisSymbolKind> {
    match kind {
        ItemKind::Function => Some(AnalysisSymbolKind::Function),
        ItemKind::Test => Some(AnalysisSymbolKind::Test),
        ItemKind::Method => Some(AnalysisSymbolKind::Method),
        ItemKind::Type => Some(AnalysisSymbolKind::Type),
        ItemKind::Enum => Some(AnalysisSymbolKind::Enum),
        ItemKind::Contract => Some(AnalysisSymbolKind::Contract),
        ItemKind::Module => Some(AnalysisSymbolKind::Module),
        ItemKind::Use => Some(AnalysisSymbolKind::Use),
        ItemKind::EnumVariant
        | ItemKind::Field
        | ItemKind::ContractNode
        | ItemKind::ContractMethodSignature
        | ItemKind::ContractEmbedding
        | ItemKind::Parameter
        | ItemKind::Statement => None,
    }
}

fn completion_kind_from_item_kind(kind: ItemKind) -> CompletionKind {
    if let Some(symbol_kind) = analysis_symbol_kind_from_item_kind(kind) {
        return completion_kind_from_symbol_kind(symbol_kind);
    }

    match kind {
        ItemKind::EnumVariant => CompletionKind::EnumMember,
        ItemKind::Field => CompletionKind::Variable,
        ItemKind::ContractNode => CompletionKind::Method,
        ItemKind::ContractMethodSignature => CompletionKind::Method,
        ItemKind::ContractEmbedding => CompletionKind::Module,
        ItemKind::Parameter => CompletionKind::Variable,
        ItemKind::Statement => CompletionKind::Text,
        ItemKind::Function
        | ItemKind::Test
        | ItemKind::Method
        | ItemKind::Type
        | ItemKind::Enum
        | ItemKind::Contract
        | ItemKind::Module
        | ItemKind::Use => unreachable!("covered by analysis_symbol_kind_from_item_kind"),
    }
}

fn completion_kind_from_symbol_kind(kind: AnalysisSymbolKind) -> CompletionKind {
    match kind {
        AnalysisSymbolKind::Function => CompletionKind::Function,
        AnalysisSymbolKind::Test => CompletionKind::Function,
        AnalysisSymbolKind::Method => CompletionKind::Method,
        AnalysisSymbolKind::Type => CompletionKind::Struct,
        AnalysisSymbolKind::Enum => CompletionKind::Enum,
        AnalysisSymbolKind::Contract => CompletionKind::Interface,
        AnalysisSymbolKind::Module => CompletionKind::Module,
        AnalysisSymbolKind::Use => CompletionKind::Module,
    }
}

fn item_kind_name(kind: ItemKind) -> &'static str {
    if let Some(symbol_kind) = analysis_symbol_kind_from_item_kind(kind) {
        return symbol_kind_name(symbol_kind);
    }

    match kind {
        ItemKind::EnumVariant => "enum variant",
        ItemKind::Field => "field",
        ItemKind::ContractNode => "contract node",
        ItemKind::ContractMethodSignature => "contract method",
        ItemKind::ContractEmbedding => "contract embedding",
        ItemKind::Parameter => "parameter",
        ItemKind::Statement => "statement",
        ItemKind::Function
        | ItemKind::Test
        | ItemKind::Method
        | ItemKind::Type
        | ItemKind::Enum
        | ItemKind::Contract
        | ItemKind::Module
        | ItemKind::Use => unreachable!("covered by analysis_symbol_kind_from_item_kind"),
    }
}

pub fn symbol_kind_name(kind: AnalysisSymbolKind) -> &'static str {
    match kind {
        AnalysisSymbolKind::Function => "function",
        AnalysisSymbolKind::Test => "test",
        AnalysisSymbolKind::Method => "method",
        AnalysisSymbolKind::Type => "type",
        AnalysisSymbolKind::Enum => "enum",
        AnalysisSymbolKind::Contract => "contract",
        AnalysisSymbolKind::Module => "module",
        AnalysisSymbolKind::Use => "use",
    }
}

fn build_document_snapshot(
    program: &Spanned<Program>,
    source_name: &str,
    source_text: &str,
    path: &Path,
    resolution: Option<Resolution>,
    assembly_module_paths: HashSet<String>,
    assembly: Option<&ProgramAssembly>,
    compile_plan: Option<&CompilePlan>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let item_docs = if let Some(res) = resolution.as_ref() {
        if let Some(asm) = assembly {
            let programs: Vec<(&Path, &Program)> = asm
                .units
                .iter()
                .map(|unit| (unit.path.as_path(), &unit.program.node))
                .collect();
            crate::doc::build_item_docs_for_resolution(res, &programs, docs_ref_links)
        } else {
            crate::doc::build_item_docs_markdown(&program.node, res, docs_ref_links)
        }
    } else {
        Vec::new()
    };

    let doc_diagnostics = resolution
        .as_ref()
        .map(|r| crate::doc::collect_doc_diagnostics(&program.node, r, source_name, source_text))
        .unwrap_or_default();
    let composition_diagnostics = super::composition::composition_diagnostics_for_program(
        program,
        compile_plan,
        source_name,
        source_text,
    )
    .unwrap_or_default();

    DocumentAnalysisSnapshot {
        program: program.clone(),
        resolution,
        item_docs,
        doc_diagnostics,
        composition_diagnostics,
        source_path: path.to_path_buf(),
        assembly_module_paths,
    }
}

/// Build an IDE snapshot from entry resolution produced by the prepare spine
/// (for example [`beskid_queries::entry_resolution_with_db`]).
pub fn build_document_analysis_from_resolution(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    resolution: Option<Resolution>,
    assembly_module_paths: HashSet<String>,
    compile_plan: Option<&CompilePlan>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        resolution,
        assembly_module_paths,
        None,
        compile_plan,
        docs_ref_links,
    )
}

pub fn build_document_analysis(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let source_path = Path::new(source_name.as_ref());
    build_document_analysis_for_resolved(
        program,
        source_name.as_ref(),
        source_text,
        source_path,
        None,
        docs_ref_links,
    )
}

/// Like [`build_document_analysis`], with optional [`ProgramAssembly`] for multi-unit docs and resolution.
pub fn build_document_analysis_for_resolved(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    assembly: Option<&ProgramAssembly>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let (resolution, assembly_module_paths) = assembly
        .and_then(|asm| {
            resolve_assembly_for_api_documentation(asm, path)
                .map(|resolution| (Some(resolution), asm.module_index.known_module_path_strings()))
        })
        .unwrap_or((None, HashSet::new()));

    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        resolution,
        assembly_module_paths,
        assembly,
        None,
        docs_ref_links,
    )
}

/// Build an IDE snapshot using project session metadata (composition diagnostics only).
///
/// For entry resolution and multi-unit docs, callers must use
/// [`beskid_queries::entry_resolution_with_db`] (or the prepare spine) and then
/// [`build_document_analysis_from_resolution`].
pub fn build_document_analysis_with_context(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    ctx: Option<&ProjectSessionHandle>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let compile_plan = ctx.and_then(|handle| handle.compile_plan.as_ref());

    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        None,
        HashSet::new(),
        None,
        compile_plan,
        docs_ref_links,
    )
}

pub fn collect_test_cases(program: &Spanned<Program>) -> Vec<TestCaseInfo> {
    let mut out = Vec::new();
    for item in &program.node.items {
        collect_test_cases_from_node(item, &mut Vec::new(), &mut out);
    }
    out
}

fn collect_test_cases_from_node(
    item: &Spanned<Node>,
    module_path: &mut Vec<String>,
    out: &mut Vec<TestCaseInfo>,
) {
    match &item.node {
        Node::TestDefinition(definition) => out.push(test_case_info(definition, module_path)),
        Node::InlineModule(module) => {
            module_path.push(module.node.name.node.name.clone());
            for nested in &module.node.items {
                collect_test_cases_from_node(nested, module_path, out);
            }
            module_path.pop();
        }
        _ => {}
    }
}

fn test_case_info(definition: &Spanned<TestDefinition>, module_path: &[String]) -> TestCaseInfo {
    let name = definition.node.name.node.name.clone();
    let qualified_name = if module_path.is_empty() {
        name.clone()
    } else {
        format!("{}::{}", module_path.join("::"), name)
    };
    let mut tags = Vec::new();
    let mut group = None;
    if let Some(meta) = &definition.node.meta {
        for entry in &meta.node.entries {
            let key = entry.node.name.node.name.as_str();
            if key == "group" {
                group = literal_string(&entry.node.value);
            } else if key == "tags" {
                tags = literal_tags(&entry.node.value);
            }
        }
    }
    let mut skip_condition = None;
    let mut skip_reason = None;
    if let Some(skip) = &definition.node.skip {
        for entry in &skip.node.entries {
            let key = entry.node.name.node.name.as_str();
            if key == "condition" {
                skip_condition = literal_bool(&entry.node.value);
            } else if key == "reason" {
                skip_reason = literal_string(&entry.node.value);
            }
        }
    }
    let (definition_line, definition_column) = definition.node.name.span.line_col_start;
    TestCaseInfo {
        name,
        qualified_name,
        tags,
        group,
        skip_condition,
        skip_reason,
        selection_start: definition.node.name.span.start,
        selection_end: definition.node.name.span.end,
        definition_line,
        definition_column,
    }
}

fn literal_string(expression: &Spanned<Expression>) -> Option<String> {
    let Expression::Literal(literal) = &expression.node else {
        return None;
    };
    crate::syntax::expressions::try_decode_string_literal(&literal.node.literal.node)
}

fn literal_tags(expression: &Spanned<Expression>) -> Vec<String> {
    literal_string(expression)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn literal_bool(expression: &Spanned<Expression>) -> Option<bool> {
    let Expression::Literal(literal) = &expression.node else {
        return None;
    };
    let Literal::Bool(value) = &literal.node.literal.node else {
        return None;
    };
    Some(*value)
}

pub fn collect_document_symbols(snapshot: &DocumentAnalysisSnapshot) -> Vec<DocumentSymbolInfo> {
    snapshot
        .program
        .node
        .items
        .iter()
        .filter_map(|item| match &item.node {
            Node::Function(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Function,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::Method(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Method,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::ExtendTypeDefinition(_) => None,
            Node::TestDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Test,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::TypeDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Type,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::EnumDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Enum,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::ContractDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Contract,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::AttributeDeclaration(_) => None,
            Node::ModuleDeclaration(definition) => {
                let segment = definition.node.path.node.segments.last()?;
                Some(DocumentSymbolInfo {
                    name: segment.node.name.node.name.clone(),
                    kind: AnalysisSymbolKind::Module,
                    selection_start: segment.span.start,
                    selection_end: segment.span.end,
                })
            }
            Node::InlineModule(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Module,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::MacroDefinition(_) => None,
            Node::HostDefinition(_) => None,
            Node::UseDeclaration(definition) => {
                if let Some(alias) = &definition.node.alias {
                    return Some(DocumentSymbolInfo {
                        name: alias.node.name.clone(),
                        kind: AnalysisSymbolKind::Use,
                        selection_start: alias.span.start,
                        selection_end: alias.span.end,
                    });
                }
                let segment = definition.node.path.node.segments.last()?;
                Some(DocumentSymbolInfo {
                    name: segment.node.name.node.name.clone(),
                    kind: AnalysisSymbolKind::Use,
                    selection_start: segment.span.start,
                    selection_end: segment.span.end,
                })
            }
        })
        .collect()
}

/// Resolved item id at `offset` for documentation routing (definitions and enclosing items).
pub fn item_id_at_offset(snapshot: &DocumentAnalysisSnapshot, offset: usize) -> Option<ItemId> {
    let resolution = snapshot.resolution.as_ref()?;
    if let Some(resolved) = resolved_value_at_offset(resolution, offset) {
        return match resolved {
            ResolvedValue::Item(item_id) => Some(*item_id),
            ResolvedValue::Local(_) => None,
        };
    }
    resolution
        .items
        .iter()
        .filter(|item| item.span.start <= offset && offset <= item.span.end)
        .min_by_key(|item| item.span.end.saturating_sub(item.span.start))
        .map(|item| item.id)
}

pub fn hover_at_offset(snapshot: &DocumentAnalysisSnapshot, offset: usize) -> Option<HoverInfo> {
    let resolution = snapshot.resolution.as_ref()?;
    if let Some(resolved) = resolved_value_at_offset(resolution, offset) {
        return match resolved {
            ResolvedValue::Item(item_id) => hover_for_item(snapshot, item_id.0),
            ResolvedValue::Local(local_id) => {
                let local = resolution.tables.local_info(*local_id)?;
                Some(HoverInfo {
                    markdown: format!("**local** `{}`", local.name),
                    location: symbol_location_for_span(
                        &snapshot.source_path,
                        local.span.start,
                        local.span.end,
                    ),
                })
            }
        };
    }
    resolution
        .items
        .iter()
        .filter(|item| item.span.start <= offset && offset <= item.span.end)
        .min_by_key(|item| item.span.end.saturating_sub(item.span.start))
        .and_then(|item| hover_for_item(snapshot, item.id.0))
}

fn hover_for_item(snapshot: &DocumentAnalysisSnapshot, item_idx: usize) -> Option<HoverInfo> {
    let item = snapshot.resolution.as_ref()?.items.get(item_idx)?;
    let mut markdown = format!("**{}** `{}`", item_kind_name(item.kind), item.name);
    if let Some(doc) = snapshot
        .item_docs
        .get(item_idx)
        .and_then(|slot| slot.as_ref())
        && !doc.markdown.trim().is_empty()
    {
        markdown.push_str("\n\n---\n\n");
        markdown.push_str(&doc.markdown);
    }
    Some(HoverInfo {
        markdown,
        location: symbol_location_for_item(item, &snapshot.source_path),
    })
}

pub fn definition_at_offset(
    snapshot: &DocumentAnalysisSnapshot,
    offset: usize,
) -> Option<DefinitionInfo> {
    let resolution = snapshot.resolution.as_ref()?;
    let resolved = resolved_value_at_offset(resolution, offset)?;
    match resolved {
        ResolvedValue::Item(item_id) => {
            let item_id = canonical_item_id(resolution, *item_id);
            let item = resolution.items.get(item_id.0)?;
            Some(DefinitionInfo {
                location: symbol_location_for_item(item, &snapshot.source_path),
            })
        }
        ResolvedValue::Local(local_id) => {
            let local = resolution.tables.local_info(*local_id)?;
            Some(DefinitionInfo {
                location: symbol_location_for_span(
                    &snapshot.source_path,
                    local.span.start,
                    local.span.end,
                ),
            })
        }
    }
}

pub fn references_at_offset(
    snapshot: &DocumentAnalysisSnapshot,
    offset: usize,
    include_declaration: bool,
) -> Vec<ReferenceInfo> {
    let Some(resolution) = snapshot.resolution.as_ref() else {
        return Vec::new();
    };

    let Some(target_resolved) = resolved_value_at_offset(resolution, offset).copied() else {
        return Vec::new();
    };
    let target = reference_target(resolution, &target_resolved);

    let mut references: Vec<ReferenceInfo> = resolution
        .tables
        .resolved_values
        .iter()
        .filter_map(|(span, resolved)| {
            if reference_targets_match(resolution, target, resolution, resolved) {
                Some(ReferenceInfo {
                    location: symbol_location_for_span(&snapshot.source_path, span.start, span.end),
                })
            } else {
                None
            }
        })
        .collect();

    if include_declaration {
        match target_resolved {
            ResolvedValue::Item(item_id) => {
                let item_id = canonical_item_id(resolution, item_id);
                if let Some(item) = resolution.items.get(item_id.0) {
                    references.push(ReferenceInfo {
                        location: symbol_location_for_item(item, &snapshot.source_path),
                    });
                }
            }
            ResolvedValue::Local(local_id) => {
                if let Some(local) = resolution.tables.local_info(local_id) {
                    references.push(ReferenceInfo {
                        location: symbol_location_for_span(
                            &snapshot.source_path,
                            local.span.start,
                            local.span.end,
                        ),
                    });
                }
            }
        }
    }

    references.sort_by_key(|reference| {
        (
            reference.location.path.clone(),
            reference.location.start,
            reference.location.end,
        )
    });
    references.dedup_by(|left, right| left.location == right.location);
    references
}

pub fn references_at_offset_workspace(
    snapshot: &DocumentAnalysisSnapshot,
    assembly: &ProgramAssembly,
    entry_path: &Path,
    offset: usize,
    include_declaration: bool,
) -> Vec<ReferenceInfo> {
    let mut references = references_at_offset(snapshot, offset, include_declaration);

    let resolution = match snapshot.resolution.as_ref() {
        Some(r) => r,
        None => return references,
    };
    let Some(target_resolved) = resolved_value_at_offset(resolution, offset).copied() else {
        return references;
    };
    let target = reference_target(resolution, &target_resolved);

    for (index, unit_hir) in assembly.hir_units.iter().enumerate() {
        if index == assembly.entry_index {
            continue;
        }
        let Ok(unit_resolution) =
            assembly
                .module_index
                .resolve_unit_hir(&unit_hir.hir, &unit_hir.path)
        else {
            continue;
        };
        for (span, resolved) in &unit_resolution.tables.resolved_values {
            if !reference_targets_match(resolution, target, &unit_resolution, resolved) {
                continue;
            }
            references.push(ReferenceInfo {
                location: symbol_location_for_span(&unit_hir.path, span.start, span.end),
            });
        }
    }

    if include_declaration
        && let ResolvedValue::Item(item_id) = target_resolved
        && let item_id = canonical_item_id(resolution, item_id)
        && let Some(item) = resolution.items.get(item_id.0)
        && item
            .source_path
            .as_ref()
            .is_some_and(|path| path != entry_path)
    {
        references.push(ReferenceInfo {
            location: symbol_location_for_item(item, entry_path),
        });
    }

    references.sort_by_key(|reference| {
        (
            reference.location.path.clone(),
            reference.location.start,
            reference.location.end,
        )
    });
    references.dedup_by(|left, right| left.location == right.location);
    references
}

fn member_access_prefix(source_text: &str, offset: usize) -> Option<(String, String)> {
    let prefix = source_text.get(..offset)?;
    let mut alias_end = offset;
    let bytes = prefix.as_bytes();
    let mut index = offset;
    while index > 0 {
        index -= 1;
        let ch = bytes[index];
        if ch.is_ascii_alphanumeric() || ch == b'_' {
            alias_end = index;
            continue;
        }
        if ch == b'.' {
            let alias_start = alias_end;
            let alias = prefix.get(alias_start..offset)?.to_string();
            if alias.is_empty()
                || !alias.as_bytes()[0].is_ascii_alphabetic() && alias.as_bytes()[0] != b'_'
            {
                return None;
            }
            let partial_start = index + 1;
            let partial = prefix.get(partial_start..offset).unwrap_or("").to_string();
            return Some((alias, partial));
        }
        return None;
    }
    None
}

fn use_path_prefix(source_text: &str, offset: usize) -> Option<String> {
    let line_start = source_text[..offset]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line = source_text.get(line_start..offset)?;
    let trimmed = line.trim_start();
    if !trimmed.starts_with("use ") {
        return None;
    }
    let path_part = trimmed.strip_prefix("use ")?.trim_start();
    if path_part.contains(';') {
        return None;
    }
    Some(path_part.to_string())
}

fn module_path_display(path: &[String]) -> String {
    path.join("::")
}

fn member_completion_candidates(
    resolution: &Resolution,
    alias: &str,
    partial: &str,
) -> Vec<CompletionInfo> {
    let Some(module_path) = resolution.module_imports.get(alias) else {
        return Vec::new();
    };
    let Some(module_id) = resolution.module_graph.module_id(module_path) else {
        return Vec::new();
    };
    let Some(module) = resolution.module_graph.module(module_id) else {
        return Vec::new();
    };
    let module_label = module_path_display(module_path);
    let partial_lower = partial.to_lowercase();

    module
        .scope
        .keys()
        .filter(|name| {
            partial.is_empty() || name.to_lowercase().starts_with(partial_lower.as_str())
        })
        .filter_map(|name| {
            let item_id = module.scope.get(name)?;
            let item = resolution.items.get(item_id.0)?;
            if !matches!(
                item.kind,
                ItemKind::Function | ItemKind::Method | ItemKind::Type | ItemKind::Enum
            ) {
                return None;
            }
            Some(CompletionInfo {
                label: name.clone(),
                kind: completion_kind_from_item_kind(item.kind),
                detail: Some(module_label.clone()),
            })
        })
        .collect()
}

fn use_path_completion_candidates(
    typed_prefix: &str,
    assembly_module_paths: &HashSet<String>,
    module_graph: &crate::resolve::ModuleGraph,
) -> Vec<CompletionInfo> {
    let typed = typed_prefix.trim();
    let typed_segments: Vec<&str> = typed.split('.').filter(|s| !s.is_empty()).collect();
    let partial = if typed.ends_with('.') {
        ""
    } else {
        typed_segments.last().copied().unwrap_or("")
    };
    let parent_path: Vec<&str> = if typed.ends_with('.') {
        typed_segments
    } else {
        typed_segments[..typed_segments.len().saturating_sub(1)].to_vec()
    };
    let partial_lower = partial.to_lowercase();

    let paths: Vec<String> = if !assembly_module_paths.is_empty() {
        assembly_module_paths.iter().cloned().collect()
    } else {
        module_graph
            .modules()
            .iter()
            .filter(|module| !module.path.is_empty())
            .map(|module| module_path_display(&module.path))
            .collect()
    };

    let mut candidates = Vec::new();
    for path in paths {
        let segments: Vec<&str> = path.split("::").collect();
        if segments.len() <= parent_path.len() {
            continue;
        }
        if parent_path
            .iter()
            .zip(segments.iter())
            .any(|(left, right)| *left != *right)
        {
            continue;
        }
        let Some(next) = segments.get(parent_path.len()) else {
            continue;
        };
        if !partial.is_empty() && !next.to_lowercase().starts_with(partial_lower.as_str()) {
            continue;
        }
        let mut completed = parent_path
            .iter()
            .map(|segment| (*segment).to_string())
            .collect::<Vec<_>>();
        completed.push((*next).to_string());
        let label = completed.join(".");
        candidates.push(CompletionInfo {
            label: label.clone(),
            kind: CompletionKind::Module,
            detail: Some(path),
        });
    }

    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label);
    candidates
}

pub fn completion_candidates(
    snapshot: &DocumentAnalysisSnapshot,
    source_text: &str,
    offset: usize,
) -> Vec<CompletionInfo> {
    if let Some((alias, partial)) = member_access_prefix(source_text, offset)
        && let Some(resolution) = snapshot.resolution.as_ref()
    {
        let members = member_completion_candidates(resolution, &alias, &partial);
        if !members.is_empty() {
            return members;
        }
    }

    if let Some(use_prefix) = use_path_prefix(source_text, offset)
        && let Some(resolution) = snapshot.resolution.as_ref()
    {
        let paths = use_path_completion_candidates(
            &use_prefix,
            &snapshot.assembly_module_paths,
            &resolution.module_graph,
        );
        if !paths.is_empty() {
            return paths;
        }
    }

    let Some(resolution) = snapshot.resolution.as_ref() else {
        return collect_document_symbols(snapshot)
            .into_iter()
            .map(|symbol| CompletionInfo {
                label: symbol.name,
                kind: completion_kind_from_symbol_kind(symbol.kind),
                detail: Some(symbol_kind_name(symbol.kind).to_string()),
            })
            .collect();
    };

    let mut candidates = Vec::new();
    for item in &resolution.items {
        candidates.push(CompletionInfo {
            label: item.name.clone(),
            kind: completion_kind_from_item_kind(item.kind),
            detail: Some(item_kind_name(item.kind).to_string()),
        });
    }
    for local in &resolution.tables.locals {
        candidates.push(CompletionInfo {
            label: local.name.clone(),
            kind: CompletionKind::Variable,
            detail: Some("local".to_string()),
        });
    }

    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
    candidates
}

#[cfg(test)]
mod reference_target_tests {
    use std::collections::HashMap;

    use crate::hir::HirVisibility;
    use crate::resolve::{
        ExportKind, ItemId, ItemInfo, ItemKind, ModuleGraph, Resolution, ResolutionTables,
        ResolvedValue, SymbolId, SymbolQualifier, SymbolRegistry, SymbolShape,
    };
    use crate::syntax::SpanInfo;

    use super::{ReferenceTarget, reference_target, reference_targets_match};

    fn span(start: usize, end: usize) -> SpanInfo {
        SpanInfo::from_byte_range_in_source("", start, end)
    }

    fn item_with_symbol(id: usize, symbol: SymbolId) -> ItemInfo {
        ItemInfo {
            id: ItemId(id),
            parent_id: None,
            name: "SharedFn".into(),
            kind: ItemKind::Function,
            visibility: HirVisibility::Public,
            span: span(0, 8),
            source_path: None,
            symbol: Some(symbol),
        }
    }

    #[test]
    fn reference_targets_match_same_symbol_different_item_ids() {
        let mut registry = SymbolRegistry::default();
        let symbol = registry.intern(SymbolQualifier {
            package: "demo".into(),
            shape: SymbolShape::ModuleItem {
                module_path: vec!["Root".into()],
                name: "SharedFn".into(),
                kind: ExportKind::Function,
            },
        });

        let entry_item_id = ItemId(0);
        let unit_item_id = ItemId(1);
        let entry_resolution = Resolution {
            items: vec![item_with_symbol(0, symbol)],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            warnings: vec![],
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: registry.clone(),
            by_symbol: HashMap::from([(symbol, entry_item_id)]),
        };
        let unit_resolution = Resolution {
            items: vec![
                ItemInfo {
                    id: ItemId(0),
                    parent_id: None,
                    name: "Other".into(),
                    kind: ItemKind::Function,
                    visibility: HirVisibility::Public,
                    span: span(0, 4),
                    source_path: None,
                    symbol: None,
                },
                item_with_symbol(1, symbol),
            ],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            warnings: vec![],
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: registry,
            by_symbol: HashMap::from([(symbol, unit_item_id)]),
        };

        let target = reference_target(&entry_resolution, &ResolvedValue::Item(entry_item_id));
        assert_eq!(target, ReferenceTarget::Symbol(symbol));

        assert!(
            reference_targets_match(
                &entry_resolution,
                target,
                &unit_resolution,
                &ResolvedValue::Item(unit_item_id),
            ),
            "same SymbolId must match across units even when ItemId differs"
        );
        assert!(!reference_targets_match(
            &entry_resolution,
            target,
            &unit_resolution,
            &ResolvedValue::Item(ItemId(99)),
        ));
    }
}
