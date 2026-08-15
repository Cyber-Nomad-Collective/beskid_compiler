use std::sync::Arc;

use beskid_analysis::services::ResolvedInput;
use beskid_queries::{
    AstNodeKey, SemanticTypeId, SyntaxGenerationId, build_typed_program, node_span, node_type, resolved_item,
    resolved_local,
};

use crate::session::{
    documentation_facts::SyntaxDocumentationFact,
    store::{SyntaxCompletion, SyntaxDefinition, SyntaxDiagnostic, SyntaxHover, SyntaxInlayHint, SyntaxSymbol},
};

use super::revisions_resolution::lockfile_digest_for_plan;

/// Syntax-only LSP facts for one prepared entry revision.
///
/// Keeping the facts named prevents lifecycle refresh paths from silently
/// reordering independent syntax-derived capabilities.
#[derive(Default)]
pub(super) struct SyntaxFacts {
    pub(super) definitions: Vec<SyntaxDefinition>,
    pub(super) hovers: Vec<SyntaxHover>,
    pub(super) symbols: Vec<SyntaxSymbol>,
    pub(super) completion: Option<SyntaxCompletion>,
    pub(super) inlay_hints: Vec<SyntaxInlayHint>,
    pub(super) documentation: Vec<SyntaxDocumentationFact>,
    pub(super) diagnostics: Vec<SyntaxDiagnostic>,
}

pub(super) fn syntax_facts_for_entry(
    db: &mut beskid_queries::BeskidDatabase,
    resolved: &ResolvedInput,
    entry_state: &beskid_queries::TypedEntryState,
) -> SyntaxFacts {
    let Some(plan) = resolved.compile_plan.as_ref() else {
        return SyntaxFacts::default();
    };
    let Some(front_end) = entry_state.typed.as_ref() else {
        return SyntaxFacts::default();
    };
    let project = db.ensure_project_session(plan, &resolved.source_path, lockfile_digest_for_plan(plan));
    // Fail closed to prepare-spine syntax authority: post-mod-rewrite entry program, never the
    // pre-rewrite ProgramAssembly units that still carry HIR compatibility state.
    let assembly = Arc::new(front_end.syntax_assembly());
    let generation = SyntaxGenerationId(entry_state.generation);
    let Ok(typed) = build_typed_program(db, project, generation, assembly) else {
        return SyntaxFacts::default();
    };
    let unit = typed.entry;
    let Some(entry) = typed.assembly.units.get(typed.assembly.entry_index) else {
        return SyntaxFacts::default();
    };
    let index = beskid_analysis::syntax_query::SyntaxIndex::from_program(&entry.program, generation);

    let mut definitions = Vec::new();
    let mut hovers = Vec::new();
    let mut inlay_hints = Vec::new();
    for metadata in index.metadata() {
        let reference = AstNodeKey { unit, generation, node: metadata.id };
        if matches!(
            metadata.kind,
            beskid_analysis::syntax_query::NodeKind::LiteralExpression
                | beskid_analysis::syntax_query::NodeKind::PathExpression
        ) && let Some(type_label) = node_type(db, reference).ok().flatten().and_then(syntax_type_label)
            && let Some(span) = node_span(db, reference).ok().flatten()
        {
            inlay_hints.push(SyntaxInlayHint { start: span.start, end: span.end, type_label: type_label.to_string() });
        }
        let local = resolved_local(db, reference).ok().flatten();
        let declaration = local
            .map(|resolved| resolved.declaration)
            .or_else(|| resolved_item(db, reference).ok().flatten().map(|resolved| resolved.declaration));
        let Some(declaration) = declaration else {
            continue;
        };
        let Some(reference_span) = node_span(db, reference).ok().flatten() else {
            continue;
        };
        let Some(declaration_span) = node_span(db, declaration).ok().flatten() else {
            continue;
        };
        let declaration_path = declaration.unit.path(db).clone();
        definitions.push(SyntaxDefinition {
            reference_start: reference_span.start,
            reference_end: reference_span.end,
            declaration_path: declaration_path.clone(),
            declaration_start: declaration_span.start,
            declaration_end: declaration_span.end,
        });
        let Some(target_unit) = typed.assembly.units.iter().find(|candidate| candidate.path == declaration_path)
        else {
            continue;
        };
        let target_index = beskid_analysis::syntax_query::SyntaxIndex::from_program(&target_unit.program, generation);
        let (location_start, location_end, name, kind) = target_index
            .node_at(&target_unit.program, declaration.node)
            .and_then(|node| {
                node.of::<beskid_analysis::syntax::FunctionDefinition>().map(|function| {
                    (function.name.span.start, function.name.span.end, function.name.node.name.clone(), "function")
                })
            })
            .unwrap_or_else(|| {
                let name = entry.source.get(reference_span.start..reference_span.end).unwrap_or_default().to_string();
                (declaration_span.start, declaration_span.end, name, "local")
            });
        if !name.is_empty() {
            hovers.push(SyntaxHover {
                reference_start: reference_span.start,
                reference_end: reference_span.end,
                markdown: format!("**{kind}** `{name}`"),
                location_path: declaration_path,
                location_start,
                location_end,
            });
        }
    }
    definitions.sort_by_key(|definition| {
        (definition.reference_start, definition.reference_end, definition.declaration_path.clone())
    });
    definitions.dedup();
    hovers.sort_by_key(|hover| (hover.reference_start, hover.reference_end, hover.location_path.clone()));
    hovers.dedup();
    inlay_hints.sort_by_key(|hint| (hint.start, hint.end, hint.type_label.clone()));
    inlay_hints.dedup();
    let completion = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::Program)
        .next()
        .map(|node| SyntaxCompletion { anchor: AstNodeKey { unit, generation, node } });
    SyntaxFacts {
        definitions,
        hovers,
        symbols: syntax_symbols_for_program(&entry.program),
        completion,
        inlay_hints,
        documentation: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn syntax_type_label(ty: SemanticTypeId) -> Option<&'static str> {
    match ty {
        SemanticTypeId::UNIT => Some("unit"),
        SemanticTypeId::BOOL => Some("bool"),
        SemanticTypeId::I32 => Some("i32"),
        SemanticTypeId::I64 => Some("i64"),
        SemanticTypeId::U8 => Some("u8"),
        SemanticTypeId::F64 => Some("f64"),
        SemanticTypeId::CHAR => Some("char"),
        SemanticTypeId::STRING => Some("string"),
        SemanticTypeId::WORD => Some("word"),
        SemanticTypeId::POINTER => Some("pointer"),
        SemanticTypeId::NEVER => Some("never"),
        _ => None,
    }
}

fn syntax_symbols_for_program(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Vec<SyntaxSymbol> {
    use beskid_analysis::services::AnalysisSymbolKind as Kind;
    use beskid_analysis::syntax::Node;
    program
        .node
        .items
        .iter()
        .filter_map(|item| {
            match &item.node {
                Node::Function(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Function, definition.node.name.span))
                }
                Node::Method(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Method, definition.node.name.span))
                }
                Node::TestDefinition(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Test, definition.node.name.span))
                }
                Node::TypeDefinition(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Type, definition.node.name.span))
                }
                Node::EnumDefinition(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Enum, definition.node.name.span))
                }
                Node::ContractDefinition(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Contract, definition.node.name.span))
                }
                Node::InlineModule(definition) => {
                    Some((definition.node.name.node.name.clone(), Kind::Module, definition.node.name.span))
                }
                Node::ModuleDeclaration(definition) => definition
                    .node
                    .path
                    .node
                    .segments
                    .last()
                    .map(|segment| (segment.node.name.node.name.clone(), Kind::Module, segment.span)),
                Node::UseDeclaration(definition) => definition
                    .node
                    .alias
                    .as_ref()
                    .map(|alias| (alias.node.name.clone(), Kind::Use, alias.span))
                    .or_else(|| {
                        definition
                            .node
                            .path
                            .node
                            .segments
                            .last()
                            .map(|segment| (segment.node.name.node.name.clone(), Kind::Use, segment.span))
                    }),
                _ => None,
            }
            .map(|(name, kind, span)| SyntaxSymbol { name, kind, start: span.start, end: span.end })
        })
        .collect()
}
