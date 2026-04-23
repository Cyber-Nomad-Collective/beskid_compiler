use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pest::Parser;
use pest::error::InputLocation;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::hir::{AstProgram, HirProgram, lower_program as lower_hir_program, normalize_program};
use crate::parser::{BeskidParser, Rule};
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::projects::{
    CompilePlan, PROJECT_FILE_NAME, PreparedProjectWorkspace, ProjectError,
    UnresolvedDependencyPolicy, WORKSPACE_FILE_NAME, WorkspacePrepareOptions,
    build_compile_plan_with_policy, discover_project_file, discover_workspace_file,
    parse_workspace_manifest, prepare_project_workspace_with_options,
};
use crate::doc::ResolvedDoc;
use crate::query::NodeRef;
use crate::resolve::{ItemKind, Resolution, ResolvedValue, Resolver};
use crate::syntax::{Expression, Literal, Node, Program, Spanned, TestDefinition};
use crate::{AnalysisOptions, builtin_rules, run_rules};

pub struct ResolvedInput {
    pub source_path: PathBuf,
    pub source: String,
    pub compile_plan: Option<CompilePlan>,
    pub prepared_workspace: Option<PreparedProjectWorkspace>,
}

fn resolve_program(program: &Spanned<Program>) -> Option<Resolution> {
    let ast: Spanned<AstProgram> = program.clone().into();
    let mut hir: Spanned<HirProgram> = lower_hir_program(&ast);
    normalize_program(&mut hir).ok()?;
    Resolver::new().resolve_program(&hir).ok()
}

fn resolved_value_at_offset<'a>(
    resolution: &'a Resolution,
    offset: usize,
) -> Option<&'a ResolvedValue> {
    resolution
        .tables
        .resolved_values
        .iter()
        .filter(|(span, _)| span.start <= offset && offset <= span.end)
        .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
        .map(|(_, resolved)| resolved)
}

fn completion_kind_from_item_kind(kind: ItemKind) -> CompletionKind {
    match kind {
        ItemKind::Function => CompletionKind::Function,
        ItemKind::Test => CompletionKind::Function,
        ItemKind::Method => CompletionKind::Method,
        ItemKind::Type => CompletionKind::Struct,
        ItemKind::Enum => CompletionKind::Enum,
        ItemKind::EnumVariant => CompletionKind::EnumMember,
        ItemKind::Contract => CompletionKind::Interface,
        ItemKind::ContractNode => CompletionKind::Method,
        ItemKind::ContractMethodSignature => CompletionKind::Method,
        ItemKind::ContractEmbedding => CompletionKind::Module,
        ItemKind::Module => CompletionKind::Module,
        ItemKind::Use => CompletionKind::Module,
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
    match kind {
        ItemKind::Function => "function",
        ItemKind::Test => "test",
        ItemKind::Method => "method",
        ItemKind::Type => "type",
        ItemKind::Enum => "enum",
        ItemKind::EnumVariant => "enum variant",
        ItemKind::Contract => "contract",
        ItemKind::ContractNode => "contract node",
        ItemKind::ContractMethodSignature => "contract method",
        ItemKind::ContractEmbedding => "contract embedding",
        ItemKind::Module => "module",
        ItemKind::Use => "use",
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

pub struct ResolvedProject {
    pub compile_plan: Option<CompilePlan>,
    pub prepared_workspace: Option<PreparedProjectWorkspace>,
}

pub fn resolve_project(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
) -> Result<ResolvedProject> {
    resolve_project_with_policy(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        UnresolvedDependencyPolicy::Error,
    )
}

pub fn resolve_project_with_policy(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
) -> Result<ResolvedProject> {
    let explicit_manifest = project
        .map(|path| resolve_project_manifest_path(path))
        .or_else(|| input.and_then(|path| infer_manifest_from_input(path)));
    let discovered_manifest = if explicit_manifest.is_none() {
        discover_from_input_or_cwd(input, workspace_member)?
    } else {
        None
    };

    let manifest_path = explicit_manifest
        .or(discovered_manifest)
        .map(|candidate| resolve_workspace_candidate(&candidate, input, workspace_member))
        .transpose()?;
    let (compile_plan, prepared_workspace) = match manifest_path {
        Some(ref manifest) => {
            let plan =
                build_compile_plan_with_policy(manifest, target, unresolved_dependency_policy)
                    .map_err(|err| anyhow::anyhow!("{}: {err}", err.code()))?;
            let workspace = prepare_project_workspace_with_options(
                &plan,
                WorkspacePrepareOptions { frozen, locked },
            )
            .map_err(|err| anyhow::anyhow!("{}: {err}", err.code()))?;
            (Some(plan), Some(workspace))
        }
        None => (None, None),
    };

    Ok(ResolvedProject {
        compile_plan,
        prepared_workspace,
    })
}

pub fn resolve_input(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
) -> Result<ResolvedInput> {
    resolve_input_with_policy(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        UnresolvedDependencyPolicy::Error,
    )
}

pub fn resolve_input_with_policy(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
) -> Result<ResolvedInput> {
    let resolved_project = resolve_project_with_policy(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        unresolved_dependency_policy,
    )?;
    let compile_plan = resolved_project.compile_plan;
    let prepared_workspace = resolved_project.prepared_workspace;
    let input_is_manifest = input
        .map(|path| infer_manifest_from_input(path).is_some())
        .unwrap_or(false);

    let source_path = match (
        input,
        input_is_manifest,
        compile_plan.as_ref(),
        prepared_workspace.as_ref(),
    ) {
        (Some(input), false, _, _) => input.clone(),
        (_, _, Some(plan), Some(workspace)) => {
            workspace.materialized_source_root.join(&plan.target.entry)
        }
        (_, _, Some(plan), None) => plan.source_root.join(&plan.target.entry),
        (_, _, None, _) => {
            return Err(anyhow::anyhow!(
                "no input file provided and no `{}` discovered",
                PROJECT_FILE_NAME
            ));
        }
    };

    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("Failed to read file: {}", source_path.display()))?;

    Ok(ResolvedInput {
        source_path,
        source,
        compile_plan,
        prepared_workspace,
    })
}

pub fn parse_program(source: &str) -> Result<Spanned<Program>> {
    parse_program_with_source_name("<memory>", source)
}

pub fn parse_program_with_source_name(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|err| {
        let diagnostic = pest_error_diagnostic(source_name, source, &err);
        anyhow::anyhow!("{:?}", miette::Report::new(diagnostic))
    })?;
    let pair = pairs
        .next()
        .ok_or_else(|| anyhow::anyhow!("No program found"))?;
    Program::parse(pair).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow::anyhow!("{:?}", miette::Report::new(diagnostic))
    })
}

pub fn analyze_program(path: &Path, source: &str) -> Result<Vec<SemanticDiagnostic>> {
    let program = parse_program_with_source_name(&path.display().to_string(), source)?;
    Ok(run_rules(
        &program.node,
        path.display().to_string(),
        source,
        &builtin_rules(),
        AnalysisOptions::default(),
    )
    .diagnostics)
}

pub fn pest_error_diagnostic(
    source_name: &str,
    source: &str,
    err: &pest::error::Error<Rule>,
) -> SemanticDiagnostic {
    let start = match err.location {
        InputLocation::Pos(pos) => pos,
        InputLocation::Span((start, _)) => start,
    };
    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        crate::syntax::SpanInfo {
            start,
            end: start.saturating_add(1),
            line_col_start: (1, 1),
            line_col_end: (1, 1),
        },
        format!("parse error: {err}"),
        "parse",
        None,
        Some("parse".to_string()),
        crate::analysis::Severity::Error,
    )
}

pub fn parse_error_diagnostic(
    source_name: &str,
    source: &str,
    err: &ParseError,
) -> SemanticDiagnostic {
    match err {
        ParseError::UnexpectedRule {
            expected,
            found,
            span,
        } => {
            let message = match expected {
                Some(rule) => format!("parse error: expected {rule:?}, found {found:?}"),
                None => format!("parse error: unexpected {found:?}"),
            };
            crate::analysis::diagnostics::make_diagnostic(
                source_name,
                source,
                *span,
                message,
                "parse",
                None,
                Some("parse".to_string()),
                crate::analysis::Severity::Error,
            )
        }
        ParseError::MissingPair { expected } => crate::analysis::diagnostics::make_diagnostic(
            source_name,
            source,
            crate::syntax::SpanInfo {
                start: 0,
                end: 0,
                line_col_start: (1, 1),
                line_col_end: (1, 1),
            },
            format!("parse error: missing {expected:?}"),
            "parse",
            None,
            Some("parse".to_string()),
            crate::analysis::Severity::Error,
        ),
        ParseError::ForbiddenImplSelfParameter { span } => {
            crate::analysis::diagnostics::make_diagnostic(
                source_name,
                source,
                *span,
                "parse error: explicit `self` parameter is not allowed in impl methods",
                "parse",
                None,
                Some("parse".to_string()),
                crate::analysis::Severity::Error,
            )
        }
    }
}

pub fn project_error_diagnostic(
    source_name: &str,
    source: &str,
    error: &ProjectError,
) -> SemanticDiagnostic {
    let (span, message): (crate::syntax::SpanInfo, String) = match error {
        ProjectError::ParseAt {
            line,
            message,
            start,
            end,
        } => {
            let span = if let (Some(s), Some(e)) = (start, end) {
                if *e > *s {
                    crate::syntax::SpanInfo::from_byte_range_in_source(source, *s, *e)
                } else {
                    crate::syntax::SpanInfo::whole_line_in_source(source, *line)
                }
            } else {
                crate::syntax::SpanInfo::whole_line_in_source(source, *line)
            };
            (span, message.clone())
        }
        _ => {
            let end = if source.is_empty() {
                0
            } else {
                1.min(source.len())
            };
            (
                crate::syntax::SpanInfo {
                    start: 0,
                    end,
                    line_col_start: (1, 1),
                    line_col_end: (1, 1),
                },
                error.to_string(),
            )
        }
    };

    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        span,
        message,
        "project",
        None,
        Some("project".to_string()),
        crate::analysis::Severity::Error,
    )
}

#[derive(Debug, Clone)]
pub struct DocumentAnalysisSnapshot {
    pub program: Spanned<Program>,
    pub resolution: Option<Resolution>,
    /// Same indexing as `resolution.items` when resolution is present; otherwise empty.
    pub item_docs: Vec<Option<ResolvedDoc>>,
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
}

pub fn build_document_analysis(program: &Spanned<Program>) -> DocumentAnalysisSnapshot {
    let resolution = resolve_program(program);
    let item_docs = resolution
        .as_ref()
        .map(|r| crate::doc::build_item_docs_markdown(&program.node, r))
        .unwrap_or_default();
    DocumentAnalysisSnapshot {
        program: program.clone(),
        resolution,
        item_docs,
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
    TestCaseInfo {
        name,
        qualified_name,
        tags,
        group,
        skip_condition,
        skip_reason,
        selection_start: definition.node.name.span.start,
        selection_end: definition.node.name.span.end,
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
    if let Some(doc) = snapshot.item_docs.get(item_idx).and_then(|slot| slot.as_ref()) {
        if !doc.markdown.trim().is_empty() {
            markdown.push_str("\n\n---\n\n");
            markdown.push_str(&doc.markdown);
        }
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

pub fn render_program_tree(program: &Spanned<Program>) -> String {
    let mut out = String::new();
    render_tree_node(NodeRef::from(&program.node), 0, &mut out);
    out
}

fn render_tree_node(node: NodeRef, indent: usize, out: &mut String) {
    let prefix = "  ".repeat(indent);
    let kind = node.node_kind();

    let extra = if let Some(ident) = node.of::<crate::syntax::Identifier>() {
        format!(" ({})", ident.name)
    } else if let Some(lit) = node.of::<crate::syntax::Literal>() {
        format!(" ({lit:?})")
    } else {
        String::new()
    };

    out.push_str(&format!("{}{:?}{}\n", prefix, kind, extra));
    node.children(|child| {
        render_tree_node(child, indent + 1, out);
    });
}

fn discover_from_input_or_cwd(
    input: Option<&PathBuf>,
    workspace_member: Option<&str>,
) -> Result<Option<PathBuf>> {
    if let Some(input) = input {
        if let Some(project_manifest) = discover_project_file(input) {
            return Ok(Some(project_manifest));
        }

        if let Some(workspace_manifest) = discover_workspace_file(input) {
            let member_manifest = resolve_project_manifest_from_workspace(
                &workspace_manifest,
                Some(input),
                workspace_member,
            )?;
            return Ok(Some(member_manifest));
        }

        return Ok(None);
    }

    let cwd = env::current_dir().ok();
    let Some(cwd) = cwd else {
        return Ok(None);
    };

    if let Some(project_manifest) = discover_project_file(&cwd) {
        return Ok(Some(project_manifest));
    }

    if let Some(workspace_manifest) = discover_workspace_file(&cwd) {
        let member_manifest =
            resolve_project_manifest_from_workspace(&workspace_manifest, None, workspace_member)?;
        return Ok(Some(member_manifest));
    }

    Ok(None)
}

fn resolve_project_manifest_path(project: &Path) -> PathBuf {
    if project.is_dir() {
        let project_manifest = project.join(PROJECT_FILE_NAME);
        if project_manifest.is_file() {
            return project_manifest;
        }

        let workspace_manifest = project.join(WORKSPACE_FILE_NAME);
        if workspace_manifest.is_file() {
            return workspace_manifest;
        }

        project_manifest
    } else {
        project.to_path_buf()
    }
}

fn resolve_workspace_candidate(
    candidate: &Path,
    input: Option<&PathBuf>,
    workspace_member: Option<&str>,
) -> Result<PathBuf> {
    let is_workspace_manifest = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == WORKSPACE_FILE_NAME);

    if is_workspace_manifest {
        resolve_project_manifest_from_workspace(candidate, input, workspace_member)
    } else {
        Ok(candidate.to_path_buf())
    }
}

fn resolve_project_manifest_from_workspace(
    workspace_manifest_path: &Path,
    input: Option<&PathBuf>,
    workspace_member: Option<&str>,
) -> Result<PathBuf> {
    let source = fs::read_to_string(workspace_manifest_path).map_err(|io_error| {
        let message = io_error.to_string();
        anyhow::anyhow!(
            "{}: failed to read workspace manifest at {}: {}",
            ProjectError::ReadManifest {
                path: workspace_manifest_path.to_path_buf(),
                source: io_error,
            }
            .code(),
            workspace_manifest_path.display(),
            message,
        )
    })?;

    let workspace_manifest = parse_workspace_manifest(&source)
        .map_err(|err| anyhow::anyhow!("{}: {err}", err.code()))?;

    let workspace_root = workspace_manifest_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "{}: invalid workspace manifest path {}",
            ProjectError::Validation("invalid workspace manifest path".to_string()).code(),
            workspace_manifest_path.display()
        )
    })?;

    let selected_member = if let Some(member_name) = workspace_member {
        workspace_manifest
            .members
            .iter()
            .find(|member| member.name == member_name)
    } else if let Some(input_path) = input {
        workspace_manifest
            .members
            .iter()
            .filter_map(|member| {
                let candidate_root = workspace_root.join(&member.path);
                if input_path.starts_with(&candidate_root) {
                    let depth = candidate_root.components().count();
                    Some((depth, member))
                } else {
                    None
                }
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, member)| member)
            .or_else(|| workspace_manifest.members.first())
    } else {
        workspace_manifest.members.first()
    }
    .ok_or_else(|| {
        anyhow::anyhow!(
            "{}: workspace manifest `{}` could not resolve member (requested={})",
            ProjectError::Validation("workspace has no members".to_string()).code(),
            workspace_manifest_path.display(),
            workspace_member.unwrap_or("<auto>")
        )
    })?;

    let member_manifest = workspace_root
        .join(&selected_member.path)
        .join(PROJECT_FILE_NAME);

    if !member_manifest.is_file() {
        return Err(anyhow::anyhow!(
            "{}: workspace member `{}` project file not found at {}",
            ProjectError::ProjectFileNotFound(member_manifest.clone()).code(),
            selected_member.name,
            member_manifest.display()
        ));
    }

    Ok(member_manifest)
}

fn infer_manifest_from_input(input: &Path) -> Option<PathBuf> {
    if input
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == PROJECT_FILE_NAME)
    {
        return Some(input.to_path_buf());
    }

    if input.extension().and_then(|ext| ext.to_str()) == Some("proj") {
        return Some(input.to_path_buf());
    }

    None
}
