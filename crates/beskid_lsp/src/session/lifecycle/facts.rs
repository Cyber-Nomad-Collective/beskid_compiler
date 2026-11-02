use std::sync::Arc;

use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::services::{ResolvedInput, parse_program_with_source_name};
use beskid_queries::{
    AstNodeKey, SemanticTypeId, SyntaxGenerationId, build_typed_program, node_span, node_type, resolved_item,
    resolved_local,
};
use pest::Parser;

use crate::session::{
    documentation_facts::SyntaxDocumentationFact,
    store::{
        BsolSemanticTokenCandidate, BsolSemanticTokenKind, SyntaxCompletion, SyntaxDefinition, SyntaxDiagnostic,
        SyntaxFix, SyntaxHover, SyntaxInlayHint, SyntaxSymbol,
    },
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
    pub(super) bsol_semantic_token_candidates: Vec<BsolSemanticTokenCandidate>,
    /// Attribute names of the current buffer's recoverable syntax.
    pub(super) attribute_names: Vec<crate::session::store::SyntaxAttributeName>,
    pub(super) completion: Option<SyntaxCompletion>,
    pub(super) inlay_hints: Vec<SyntaxInlayHint>,
    pub(super) documentation: Vec<SyntaxDocumentationFact>,
    pub(super) diagnostics: Vec<SyntaxDiagnostic>,
    /// Mod-origin quick-fixes collected from the prepare spine (empty on structural/manifest paths).
    pub(super) fixes: Vec<SyntaxFix>,
    /// The client sent a newer text before the analysis began; these facts are empty and must
    /// not be stored.
    pub(super) superseded: bool,
    /// The project preparation of the text succeeded, so `diagnostics` are the document's
    /// judged diagnostics (not a structural fallback or a preparation failure).
    pub(super) prepared: bool,
}

pub(super) fn syntax_facts_for_assembly(
    db: &mut beskid_queries::BeskidDatabase,
    resolved: &ResolvedInput,
    assembly: Arc<ProgramAssembly>,
) -> SyntaxFacts {
    let typed = typed_program_for_assembly(db, resolved, Arc::clone(&assembly));
    let entry_path = assembly.units.get(assembly.entry_index).map(|entry| entry.path.clone()).unwrap_or_default();
    unit_syntax_facts(db, typed.as_ref(), &assembly, &entry_path, &resolved.source_path, &resolved.source)
}

/// Type `assembly` once in the project session that owns its registered syntax (the prepare spine
/// registered it moments ago; a second session would be rejected as a source-unit reassignment).
/// `None` without a compile plan or when typing fails.
pub(super) fn typed_program_for_assembly(
    db: &mut beskid_queries::BeskidDatabase,
    resolved: &ResolvedInput,
    assembly: Arc<ProgramAssembly>,
) -> Option<beskid_queries::TypedProgram> {
    let plan = resolved.compile_plan.as_ref()?;
    let generation = SyntaxGenerationId(assembly.generation.0);
    beskid_queries::project_session_for_planned_syntax_assembly(
        db,
        &assembly,
        plan,
        &resolved.source_path,
        lockfile_digest_for_plan(plan),
    )
    .and_then(|project| build_typed_program(db, project, generation, assembly))
    .ok()
}

/// Facts for the unit at `unit_path` of a typed assembly: the entry for an open buffer, or any
/// unit when the workspace index types a whole project once. `source_name`/`source` are the
/// unit's file and text.
pub(super) fn unit_syntax_facts(
    db: &mut beskid_queries::BeskidDatabase,
    typed: Option<&beskid_queries::TypedProgram>,
    assembly: &Arc<ProgramAssembly>,
    unit_path: &std::path::Path,
    source_name: &std::path::Path,
    source: &str,
) -> SyntaxFacts {
    let dependency_surface = parse_program_with_source_name(&source_name.display().to_string(), source)
        .map(|program| beskid_queries::completion_dependency_surface_for_program(assembly, &program))
        .unwrap_or_else(|_| beskid_queries::completion_dependency_surface_for_assembly(assembly));
    let recoverable_symbols = assembly
        .units
        .iter()
        .find(|unit| unit.path == unit_path)
        .map(|unit| syntax_symbols_for_program(&unit.program))
        .unwrap_or_default();
    // Fail closed to prepare-spine syntax authority: post-mod-rewrite programs, never the
    // pre-rewrite ProgramAssembly units that still carry HIR compatibility state.
    let generation = SyntaxGenerationId(assembly.generation.0);
    let Some(typed) = typed else {
        return SyntaxFacts {
            symbols: recoverable_symbols,
            completion: (!dependency_surface.is_empty()).then_some(SyntaxCompletion {
                entry_anchor: None,
                dependency_surface,
                attributes: Arc::from([]),
                member_access: Arc::from([]),
            }),
            ..SyntaxFacts::default()
        };
    };
    let Some(entry) = typed.assembly.units.iter().find(|unit| unit.path == unit_path) else {
        return SyntaxFacts::default();
    };
    let unit = if typed.assembly.units.get(typed.assembly.entry_index).is_some_and(|e| e.path == unit_path) {
        typed.entry
    } else {
        beskid_queries::SourceUnitId::new(db, entry.path.clone())
    };
    let index = beskid_analysis::syntax_query::SyntaxIndex::from_program(&entry.program, generation);

    let mut definitions = Vec::new();
    let mut hovers = Vec::new();
    let mut inlay_hints = Vec::new();
    // The syntax index of each unit a reference resolves into, built once per unit (not once per
    // reference: a file with many references into a large unit built that unit's index each time).
    let mut target_indices = std::collections::HashMap::<std::path::PathBuf, _>::new();
    for metadata in index.metadata() {
        let reference = AstNodeKey { unit, generation, node: metadata.id };
        if matches!(
            metadata.kind,
            beskid_analysis::syntax_query::NodeKind::LiteralExpression
                | beskid_analysis::syntax_query::NodeKind::PathExpression
        ) && let Some(type_label) = node_type(db, reference).ok().flatten().and_then(syntax_type_label)
            && let Some(span) = node_span(db, reference).ok().flatten()
            // Arguments the compiler filled for omitted optional parameters have a zero-width
            // span and no source text: they get no hint.
            && span.start < span.end
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
        let Some(target_unit) = typed.assembly.units.iter().find(|candidate| candidate.path == declaration_path) else {
            continue;
        };
        let target_index = target_indices.entry(declaration_path.clone()).or_insert_with(|| {
            beskid_analysis::syntax_query::SyntaxIndex::from_program(&target_unit.program, generation)
        });
        // A function shows its signature (with default values), which signature help reuses as
        // its label.
        let (location_start, location_end, name, kind) = target_index
            .node_at(&target_unit.program, declaration.node)
            .and_then(|node| {
                node.of::<beskid_analysis::syntax::FunctionDefinition>().map(|function| {
                    (
                        function.name.span.start,
                        function.name.span.end,
                        beskid_analysis::doc::function_signature_label(function),
                        "function",
                    )
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
    let (attribute_definitions, attribute_hovers) =
        crate::session::attribute_facts::attribute_reference_facts(db, unit, generation, entry, &typed.assembly.units);
    definitions.extend(attribute_definitions);
    hovers.extend(attribute_hovers);
    hovers.extend(crate::session::attribute_facts::transfer_class_hovers(db, unit, generation, entry));
    hovers.extend(crate::session::attribute_facts::ownership_hovers(db, unit, generation, entry));
    let attributes =
        crate::session::attribute_facts::attribute_completion_catalogue(db, unit, generation, &typed.assembly.units);
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
        .map(|node| AstNodeKey { unit, generation, node })
        .map(|entry_anchor| SyntaxCompletion {
            entry_anchor: Some(entry_anchor),
            dependency_surface,
            attributes,
            member_access: Arc::from([]),
        });
    SyntaxFacts {
        definitions,
        hovers,
        symbols: syntax_symbols_for_program(&entry.program),
        bsol_semantic_token_candidates: Vec::new(),
        attribute_names: Vec::new(),
        completion,
        inlay_hints,
        documentation: Vec::new(),
        diagnostics: Vec::new(),
        fixes: Vec::new(),
        superseded: false,
        prepared: false,
    }
}

pub(crate) fn bsol_semantic_token_candidates(text: &str) -> Vec<BsolSemanticTokenCandidate> {
    if bsol::parse_bsol_document(text).is_err() {
        return Vec::new();
    }
    let Ok(pairs) = bsol::BsolParser::parse(bsol::Rule::document, text) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for pair in pairs {
        collect_bsol_semantic_token_candidates(pair, &mut candidates);
    }
    candidates
}

fn collect_bsol_semantic_token_candidates(
    pair: pest::iterators::Pair<'_, bsol::Rule>,
    out: &mut Vec<BsolSemanticTokenCandidate>,
) {
    match pair.as_rule() {
        bsol::Rule::block_kind => {
            let span = pair.as_span();
            out.push(BsolSemanticTokenCandidate {
                start: span.start(),
                end: span.end(),
                kind: BsolSemanticTokenKind::Namespace,
            });
        }
        bsol::Rule::assignment | bsol::Rule::map_entry => {
            let mut is_key = true;
            for child in pair.into_inner() {
                if is_key && child.as_rule() == bsol::Rule::ident {
                    let span = child.as_span();
                    out.push(BsolSemanticTokenCandidate {
                        start: span.start(),
                        end: span.end(),
                        kind: BsolSemanticTokenKind::Property,
                    });
                    is_key = false;
                    continue;
                }
                collect_bsol_semantic_token_candidates(child, out);
            }
        }
        _ => {
            for child in pair.into_inner() {
                collect_bsol_semantic_token_candidates(child, out);
            }
        }
    }
}

fn syntax_type_label(ty: SemanticTypeId) -> Option<&'static str> {
    match ty {
        SemanticTypeId::UNIT => Some("unit"),
        SemanticTypeId::BOOL => Some("bool"),
        SemanticTypeId::I8 => Some("i8"),
        SemanticTypeId::I16 => Some("i16"),
        SemanticTypeId::U16 => Some("u16"),
        SemanticTypeId::U64 => Some("u64"),
        SemanticTypeId::F32 => Some("f32"),
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

pub(crate) fn syntax_symbols_for_program(
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

#[cfg(test)]
mod tests {
    use super::syntax_symbols_for_program;

    #[test]
    fn recoverable_partial_expression_keeps_current_outline_symbols() {
        let source = "use Core.Output;\ni32 FreshMain() {\n    Output.Wri\n    return 0;\n}";
        let program = beskid_analysis::services::parse_program_with_source_name("Main.bd", source)
            .expect("the editor parser should recover an incomplete member expression");

        let names = syntax_symbols_for_program(&program).into_iter().map(|symbol| symbol.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["Output", "FreshMain"]);
    }
}
