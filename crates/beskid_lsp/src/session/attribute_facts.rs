//! Attribute facts for one buffer revision: name spans for semantic tokens, resolved hover and
//! definition facts, and the owned attribute catalogue used by completion.
//!
//! Resolution goes through `beskid_queries::resolve_attribute_use`, the same authority that
//! judges E1510, so the editor and the compiler agree on what an attribute name denotes and on
//! which targets it admits.

use std::path::Path;
use std::sync::Arc;

use beskid_analysis::projects::SourceUnit;
use beskid_analysis::syntax::{Attribute, AttributeDeclaration, Program, Spanned, SyntaxGenerationId};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{AstNodeKey, AttributeResolution, SourceUnitId, node_span, resolve_attribute_use};

use crate::session::store::{AttributeCompletionEntry, SyntaxAttributeName, SyntaxDefinition, SyntaxHover};

/// Every attribute name of `program`: each attribute use at any declaration site, and the
/// declared name of each attribute declaration. Syntax-only, so it works on a recovered parse.
pub(crate) fn attribute_names_for_program(program: &Spanned<Program>) -> Vec<SyntaxAttributeName> {
    let index = SyntaxIndex::from_program(program, SyntaxGenerationId(0));
    let mut names = Vec::new();
    for node in index.ids_of_kind(NodeKind::Attribute) {
        if let Some(attribute) = index.node_at(program, node).and_then(|node| node.of::<Attribute>()) {
            names.push(SyntaxAttributeName {
                start: attribute.name.span.start,
                end: attribute.name.span.end,
                declaration: false,
            });
        }
    }
    for node in index.ids_of_kind(NodeKind::AttributeDeclaration) {
        if let Some(declaration) = index.node_at(program, node).and_then(|node| node.of::<AttributeDeclaration>()) {
            names.push(SyntaxAttributeName {
                start: declaration.name.span.start,
                end: declaration.name.span.end,
                declaration: true,
            });
        }
    }
    names.sort_by_key(|name| (name.start, name.end));
    names.dedup();
    names
}

/// Markdown hover body for an attribute named `name` with `targets`.
pub(crate) fn attribute_hover_markdown(name: &str, targets: &[&str], recognized: bool) -> String {
    let targets = targets.iter().map(|target| format!("`{target}`")).collect::<Vec<_>>().join(", ");
    let origin = if recognized { " (compiler-recognized)" } else { "" };
    format!("**attribute** `{name}`{origin}\n\ntargets: {targets}")
}

/// Hover and definition facts for every attribute use of the entry unit.
pub(crate) fn attribute_reference_facts(
    db: &beskid_queries::BeskidDatabase,
    unit: SourceUnitId,
    generation: beskid_queries::SyntaxGenerationId,
    entry: &SourceUnit,
    units: &[SourceUnit],
) -> (Vec<SyntaxDefinition>, Vec<SyntaxHover>) {
    let root = AstNodeKey { unit, generation, node: beskid_analysis::syntax::AstNodeId(0) };
    let index = SyntaxIndex::from_program(&entry.program, generation);
    let mut definitions = Vec::new();
    let mut hovers = Vec::new();
    for node in index.ids_of_kind(NodeKind::Attribute) {
        let Some(attribute) = index.node_at(&entry.program, node).and_then(|node| node.of::<Attribute>()) else {
            continue;
        };
        let name = attribute.name.node.name.as_str();
        let reference = attribute.name.span;
        match resolve_attribute_use(db, root, name) {
            AttributeResolution::Declared { declaration, targets, recognized } => {
                let declaration_path = declaration.unit.path(db).clone();
                let Some(declaration_span) = node_span(db, declaration).ok().flatten() else {
                    continue;
                };
                definitions.push(SyntaxDefinition {
                    reference_start: reference.start,
                    reference_end: reference.end,
                    declaration_path: declaration_path.clone(),
                    declaration_start: declaration_span.start,
                    declaration_end: declaration_span.end,
                });
                let (location_start, location_end) =
                    declaration_name_span(units, &declaration_path, generation, declaration)
                        .unwrap_or((declaration_span.start, declaration_span.end));
                let targets = targets.iter().map(AsRef::as_ref).collect::<Vec<_>>();
                hovers.push(SyntaxHover {
                    reference_start: reference.start,
                    reference_end: reference.end,
                    markdown: attribute_hover_markdown(name, &targets, recognized),
                    location_path: declaration_path,
                    location_start,
                    location_end,
                });
            }
            AttributeResolution::Recognized { targets } => {
                hovers.push(SyntaxHover {
                    reference_start: reference.start,
                    reference_end: reference.end,
                    markdown: attribute_hover_markdown(name, targets, true),
                    location_path: entry.path.clone(),
                    location_start: reference.start,
                    location_end: reference.end,
                });
            }
            AttributeResolution::Undeclared => {}
        }
    }
    (definitions, hovers)
}

fn declaration_name_span(
    units: &[SourceUnit],
    path: &Path,
    generation: beskid_queries::SyntaxGenerationId,
    declaration: AstNodeKey,
) -> Option<(usize, usize)> {
    let unit = units.iter().find(|unit| unit.path == path)?;
    let index = SyntaxIndex::from_program(&unit.program, generation);
    let declaration =
        index.node_at(&unit.program, declaration.node).and_then(|node| node.of::<AttributeDeclaration>())?;
    Some((declaration.name.span.start, declaration.name.span.end))
}

/// The attributes visible from the entry unit, each with the target list E1510 checks against.
///
/// Names come from every assembled unit's attribute declarations plus the compiler-recognized
/// attributes; each is resolved from the entry unit so shadowing and target unions match the
/// compiler.
/// Hover facts on the name of every type and enum declared in `entry`, naming its transfer
/// class (requirement "Transfer class query"). A generic declaration's class is decided per
/// instantiation, so its hover says so instead of guessing.
pub(crate) fn transfer_class_hovers(
    db: &beskid_queries::BeskidDatabase,
    unit: SourceUnitId,
    generation: beskid_queries::SyntaxGenerationId,
    entry: &SourceUnit,
) -> Vec<SyntaxHover> {
    let index = SyntaxIndex::from_program(&entry.program, generation);
    let mut hovers = Vec::new();
    for kind in [NodeKind::TypeDefinition, NodeKind::EnumDefinition] {
        for node in index.ids_of_kind(kind) {
            let Some(found) = index.node_at(&entry.program, node) else { continue };
            let (keyword, name, generic) = if let Some(definition) = found.of::<beskid_analysis::syntax::TypeDefinition>() {
                ("type", &definition.name, !definition.generics.is_empty())
            } else if let Some(definition) = found.of::<beskid_analysis::syntax::EnumDefinition>() {
                ("enum", &definition.name, !definition.generics.is_empty())
            } else {
                continue;
            };
            let class = if generic {
                "decided per instantiation".to_string()
            } else {
                match beskid_queries::nominal_transfer_class(db, AstNodeKey { unit, generation, node }, &[]) {
                    Ok(class) => class.to_string(),
                    Err(_) => continue,
                }
            };
            hovers.push(SyntaxHover {
                reference_start: name.span.start,
                reference_end: name.span.end,
                markdown: format!("**{keyword}** `{}`\n\ntransfer class: {class}", name.node.name),
                location_path: entry.path.clone(),
                location_start: name.span.start,
                location_end: name.span.end,
            });
        }
    }
    hovers
}

/// Ownership hovers (0.6.2, "Code actions and diagnostic publication for IntelliSense"): every
/// binding of Moved class (and every `Fiber` handle) names `owning` or `borrowed` at its
/// declaration; a `[Captures]` parameter or field names its bound; the arrow of a lambda passed
/// to a `[Captures]` parameter lists each capture with its transfer class.
pub(crate) fn ownership_hovers(
    db: &beskid_queries::BeskidDatabase,
    unit: SourceUnitId,
    generation: beskid_queries::SyntaxGenerationId,
    entry: &SourceUnit,
) -> Vec<SyntaxHover> {
    let index = SyntaxIndex::from_program(&entry.program, generation);
    let mut hovers = Vec::new();
    let mut push = |start: usize, end: usize, markdown: String| {
        hovers.push(SyntaxHover {
            reference_start: start,
            reference_end: end,
            markdown,
            location_path: entry.path.clone(),
            location_start: start,
            location_end: end,
        });
    };
    for kind in [NodeKind::FunctionDefinition, NodeKind::MethodDefinition, NodeKind::TestDefinition, NodeKind::LambdaExpression] {
        for node in index.ids_of_kind(kind) {
            let Ok(Some(ownership)) = beskid_queries::affine_ownership(db, AstNodeKey { unit, generation, node }) else {
                continue;
            };
            for binding in ownership.bindings.iter() {
                let Some(span) = index.node_at(&entry.program, binding.declaration.node).and_then(|node| node.span()) else {
                    continue;
                };
                let state = if binding.owning { "owning" } else { "borrowed" };
                let class = if binding.fiber { "Never (`Fiber` handle)" } else { "Moved" };
                push(span.start, span.end, format!("`{}`: {state}\n\ntransfer class: {class}", binding.name));
            }
        }
    }
    for kind in [NodeKind::Parameter, NodeKind::Field] {
        for node in index.ids_of_kind(kind) {
            let Some(bound) = beskid_queries::declared_captures_bound(db, AstNodeKey { unit, generation, node }) else {
                continue;
            };
            let Some(found) = index.node_at(&entry.program, node) else { continue };
            let name = found
                .of::<beskid_analysis::syntax::Parameter>()
                .map(|parameter| &parameter.name)
                .or_else(|| found.of::<beskid_analysis::syntax::Field>().map(|field| &field.name));
            if let Some(name) = name {
                push(name.span.start, name.span.end, format!("`{}`\n\ncapture bound: {bound}", name.node.name));
            }
        }
    }
    for node in index.ids_of_kind(NodeKind::LambdaExpression) {
        let Some(captures) = beskid_queries::bounded_lambda_captures(db, AstNodeKey { unit, generation, node }) else {
            continue;
        };
        let Some(lambda) =
            index.node_at(&entry.program, node).and_then(|node| node.of::<beskid_analysis::syntax::LambdaExpression>())
        else {
            continue;
        };
        let Some(span) = index.node_at(&entry.program, node).and_then(|node| node.span()) else { continue };
        let Some(arrow) = entry
            .source
            .get(span.start..lambda.body.span.start)
            .and_then(|text| text.rfind("=>"))
            .map(|offset| span.start + offset)
        else {
            continue;
        };
        let mut markdown = format!("lambda with capture bound `{}`\n\ncaptures:", captures.bound.name());
        for (name, type_name, class) in &captures.captures {
            markdown.push_str(&format!("\n- `{name}: {type_name} ({class})`"));
        }
        if captures.captures.is_empty() {
            markdown.push_str(" none");
        }
        push(arrow, arrow + 2, markdown);
    }
    hovers
}

pub(crate) fn attribute_completion_catalogue(
    db: &beskid_queries::BeskidDatabase,
    unit: SourceUnitId,
    generation: beskid_queries::SyntaxGenerationId,
    units: &[SourceUnit],
) -> Arc<[AttributeCompletionEntry]> {
    let root = AstNodeKey { unit, generation, node: beskid_analysis::syntax::AstNodeId(0) };
    let mut names = Vec::<Arc<str>>::new();
    for source in units {
        let source_unit = SourceUnitId::new(db, source.path.clone());
        let unit_root = AstNodeKey { unit: source_unit, generation, node: beskid_analysis::syntax::AstNodeId(0) };
        let Ok(Some(surface)) = beskid_queries::unit_attribute_surface(db, unit_root) else {
            continue;
        };
        for declaration in surface.declarations.iter() {
            if !names.contains(&declaration.name) {
                names.push(declaration.name.clone());
            }
        }
    }
    for recognized in beskid_analysis::attribute_targets::COMPILER_RECOGNIZED_ATTRIBUTES {
        let name: Arc<str> = Arc::from(recognized.name);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let mut entries = names
        .into_iter()
        .filter_map(|name| {
            let resolution = resolve_attribute_use(db, root, &name);
            let recognized = resolution.is_recognized();
            let targets: Arc<[Arc<str>]> = match resolution {
                AttributeResolution::Declared { targets, .. } => targets.into(),
                AttributeResolution::Recognized { targets } => {
                    targets.iter().map(|target| Arc::from(*target)).collect()
                }
                AttributeResolution::Undeclared => return None,
            };
            Some(AttributeCompletionEntry { name, targets, recognized })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries.into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::Arc;

    use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::parse_program_with_source_name;
    use beskid_queries::{BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, build_typed_program};
    use tower_lsp_server::ls_types::{GotoDefinitionResponse, HoverContents, Uri};

    use super::{attribute_completion_catalogue, attribute_reference_facts};
    use crate::session::store::Document;

    const SOURCE: &str = "pub attribute Trace(Parameter) { }\n\
pub attribute Pure(LambdaExpression) { }\n\
i64 Add([Trace] i64 left, i64 right) {\n    let square = [Pure] (i64 x) => x * x;\n    return left + right;\n}\n\
[Immutable] pub type Job { string id, i64 size }\n\
pub type Cache { i64[] entries }\n\
[Isolated] pub type Box<T> { T item }\n\
i64 Checksum(u8[] data) { return 0; }\n\
unit Keep(move u8[] block) { return; }\n";

    fn typed_fixture() -> (BeskidDatabase, SourceUnitId, SyntaxGenerationId, Arc<ProgramAssembly>) {
        let root = PathBuf::from("/tmp/attribute-facts/src");
        let main_path = root.join("Main.bd");
        let program = expand_program(
            parse_program_with_source_name(main_path.to_str().expect("utf8 path"), SOURCE).expect("source parses"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        );
        let generation = SyntaxGenerationId(3);
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: root.clone() },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: main_path.display().to_string(),
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: SOURCE.to_string(),
                program,
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            generation,
        ));
        let mut db = BeskidDatabase::default();
        let unit = SourceUnitId::new(&db, main_path.clone());
        let project = ProjectSession::new(
            &db,
            root.parent().expect("project root").to_path_buf(),
            main_path,
            "App".to_string(),
            "lock".to_string(),
        );
        db.ensure_file_text(unit.path(&db).clone(), SOURCE.to_string());
        build_typed_program(&mut db, project, generation, assembly.clone()).expect("typed syntax program");
        (db, unit, generation, assembly)
    }

    fn document(
        definitions: Vec<crate::session::store::SyntaxDefinition>,
        hovers: Vec<crate::session::store::SyntaxHover>,
    ) -> Document {
        Document {
            version: 1,
            text: SOURCE.to_string(),
            syntax_definitions: definitions,
            syntax_hovers: hovers,
            syntax_symbols: Vec::new(),
            bsol_semantic_token_candidates: Vec::new(),
            syntax_attribute_names: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        }
    }

    #[test]
    fn hover_on_a_parameter_attribute_shows_its_declaration_targets() {
        let (db, unit, generation, assembly) = typed_fixture();
        let entry = &assembly.units[0];
        let (definitions, hovers) = attribute_reference_facts(&db, unit, generation, entry, &assembly.units);
        let doc = document(definitions, hovers);
        let uri = Uri::from_str("file:///tmp/attribute-facts/src/Main.bd").expect("uri");

        let offset = SOURCE.find("[Trace]").expect("parameter attribute") + 2;
        let hover = crate::features::hover::handler::handle_hover(&uri, &doc, offset).expect("attribute hover");
        let HoverContents::Markup(contents) = hover.contents else {
            panic!("expected markdown hover");
        };
        assert!(contents.value.contains("**attribute** `Trace`"), "{}", contents.value);
        assert!(contents.value.contains("targets: `Parameter`"), "{}", contents.value);

        let response =
            crate::features::definition::handler::handle_definition(&uri, &doc, offset).expect("attribute definition");
        let GotoDefinitionResponse::Scalar(location) = response else {
            panic!("expected one definition location");
        };
        assert_eq!(location.range.start.line, 0, "Trace is declared on the first line: {location:?}");

        let lambda = SOURCE.find("[Pure]").expect("lambda attribute") + 2;
        let hover = crate::features::hover::handler::handle_hover(&uri, &doc, lambda).expect("lambda attribute hover");
        let HoverContents::Markup(contents) = hover.contents else {
            panic!("expected markdown hover");
        };
        assert!(contents.value.contains("targets: `LambdaExpression`"), "{}", contents.value);
        let GotoDefinitionResponse::Scalar(location) =
            crate::features::definition::handler::handle_definition(&uri, &doc, lambda).expect("definition")
        else {
            panic!("expected one definition location");
        };
        assert_eq!(location.range.start.line, 1, "Pure is declared on the second line: {location:?}");
    }

    #[test]
    fn hover_on_a_type_name_shows_its_transfer_class() {
        let (db, unit, generation, assembly) = typed_fixture();
        let hovers = super::transfer_class_hovers(&db, unit, generation, &assembly.units[0]);
        let job = hovers.iter().find(|hover| hover.markdown.contains("`Job`")).expect("Job hover");
        assert!(job.markdown.contains("transfer class: Shared"), "{}", job.markdown);
        let cache = hovers.iter().find(|hover| hover.markdown.contains("`Cache`")).expect("Cache hover");
        assert!(cache.markdown.contains("transfer class: Never (no marker attribute)"), "{}", cache.markdown);
        let boxed = hovers.iter().find(|hover| hover.markdown.contains("`Box`")).expect("Box hover");
        assert!(boxed.markdown.contains("decided per instantiation"), "{}", boxed.markdown);
    }

    #[test]
    fn hover_on_a_moved_binding_names_owning_or_borrowed() {
        let (db, unit, generation, assembly) = typed_fixture();
        let hovers = super::ownership_hovers(&db, unit, generation, &assembly.units[0]);
        let data = hovers.iter().find(|hover| hover.markdown.starts_with("`data`")).expect("data hover");
        assert!(data.markdown.contains("borrowed") && data.markdown.contains("transfer class: Moved"), "{}", data.markdown);
        let block = hovers.iter().find(|hover| hover.markdown.starts_with("`block`")).expect("block hover");
        assert!(block.markdown.contains("owning"), "{}", block.markdown);
    }

    #[test]
    fn catalogue_resolves_buffer_and_recognized_attributes() {
        let (db, unit, generation, assembly) = typed_fixture();
        let catalogue = attribute_completion_catalogue(&db, unit, generation, &assembly.units);
        let trace = catalogue.iter().find(|entry| entry.name.as_ref() == "Trace").expect("Trace");
        assert_eq!(trace.targets.iter().map(AsRef::as_ref).collect::<Vec<&str>>(), vec!["Parameter"]);
        assert!(!trace.recognized);
        let captures = catalogue.iter().find(|entry| entry.name.as_ref() == "Captures").expect("Captures");
        assert!(captures.recognized);
    }
}
