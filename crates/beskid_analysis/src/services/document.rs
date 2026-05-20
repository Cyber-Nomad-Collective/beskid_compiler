use std::borrow::ToOwned;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::doc::DocRefLinkContext;
use crate::doc::ResolvedDoc;
use crate::hir::{AstProgram, HirProgram, lower_program as lower_hir_program, normalize_program};
use crate::resolve::{ItemKind, Resolution, ResolvedValue, Resolver};
use crate::syntax::{Expression, Literal, Node, Program, Spanned, TestDefinition};

#[derive(Debug, Clone)]
pub struct DocumentAnalysisSnapshot {
    pub program: Spanned<Program>,
    pub resolution: Option<Resolution>,
    /// Same indexing as `resolution.items` when resolution is present; otherwise empty.
    pub item_docs: Vec<Option<ResolvedDoc>>,
    /// Documentation validation (codes `W161x`); empty when resolution failed.
    pub doc_diagnostics: Vec<SemanticDiagnostic>,
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
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct DefinitionInfo {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ReferenceInfo {
    pub start: usize,
    pub end: usize,
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

fn resolve_program(program: &Spanned<Program>) -> Option<Resolution> {
    let ast: Spanned<AstProgram> = program.clone().into();
    let mut hir: Spanned<HirProgram> = lower_hir_program(&ast);
    normalize_program(&mut hir).ok()?;
    Resolver::new().resolve_program(&hir).ok()
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

pub fn build_document_analysis(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let resolution = resolve_program(program);
    let item_docs = resolution
        .as_ref()
        .map(|r| crate::doc::build_item_docs_markdown(&program.node, r, docs_ref_links))
        .unwrap_or_default();
    let doc_diagnostics = resolution
        .as_ref()
        .map(|r| {
            crate::doc::collect_doc_diagnostics(&program.node, r, source_name.as_ref(), source_text)
        })
        .unwrap_or_default();
    DocumentAnalysisSnapshot {
        program: program.clone(),
        resolution,
        item_docs,
        doc_diagnostics,
    }
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
    let Literal::String(raw) = &literal.node.literal.node else {
        return None;
    };
    Some(
        raw.strip_prefix('"')
            .and_then(|trimmed| trimmed.strip_suffix('"'))
            .unwrap_or(raw)
            .to_string(),
    )
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

pub fn hover_at_offset(snapshot: &DocumentAnalysisSnapshot, offset: usize) -> Option<HoverInfo> {
    let resolution = snapshot.resolution.as_ref()?;
    if let Some(resolved) = resolved_value_at_offset(resolution, offset) {
        return match resolved {
            ResolvedValue::Item(item_id) => hover_for_item(snapshot, item_id.0),
            ResolvedValue::Local(local_id) => {
                let local = resolution.tables.local_info(*local_id)?;
                Some(HoverInfo {
                    markdown: format!("**local** `{}`", local.name),
                    start: local.span.start,
                    end: local.span.end,
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
        start: item.span.start,
        end: item.span.end,
    })
}

pub fn definition_at_offset(
    snapshot: &DocumentAnalysisSnapshot,
    offset: usize,
) -> Option<DefinitionInfo> {
    let resolved = resolved_value_at_offset(snapshot.resolution.as_ref()?, offset)?;
    match resolved {
        ResolvedValue::Item(item_id) => {
            let span = snapshot.resolution.as_ref()?.items.get(item_id.0)?.span;
            Some(DefinitionInfo {
                start: span.start,
                end: span.end,
            })
        }
        ResolvedValue::Local(local_id) => {
            let span = snapshot
                .resolution
                .as_ref()?
                .tables
                .local_info(*local_id)?
                .span;
            Some(DefinitionInfo {
                start: span.start,
                end: span.end,
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

    let Some(target) = resolved_value_at_offset(resolution, offset).copied() else {
        return Vec::new();
    };

    let mut references: Vec<ReferenceInfo> = resolution
        .tables
        .resolved_values
        .iter()
        .filter_map(|(span, resolved)| {
            if *resolved == target {
                Some(ReferenceInfo {
                    start: span.start,
                    end: span.end,
                })
            } else {
                None
            }
        })
        .collect();

    if include_declaration {
        match target {
            ResolvedValue::Item(item_id) => {
                if let Some(item) = resolution.items.get(item_id.0) {
                    references.push(ReferenceInfo {
                        start: item.span.start,
                        end: item.span.end,
                    });
                }
            }
            ResolvedValue::Local(local_id) => {
                if let Some(local) = resolution.tables.local_info(local_id) {
                    references.push(ReferenceInfo {
                        start: local.span.start,
                        end: local.span.end,
                    });
                }
            }
        }
    }

    references.sort_by_key(|reference| (reference.start, reference.end));
    references.dedup_by(|left, right| left.start == right.start && left.end == right.end);
    references
}

pub fn completion_candidates(snapshot: &DocumentAnalysisSnapshot) -> Vec<CompletionInfo> {
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
