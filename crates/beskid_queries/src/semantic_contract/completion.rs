//! Focused semantic-contract implementation cluster.

use super::*;

/// Enumerate exact syntax-backed callable completion candidates for one current generation.
///
/// Generation-safe completion facts for lexical locals, types, imported members, and
/// explicitly annotated nominal receivers. Inferred receivers remain unavailable.
pub fn completion_candidates(
    db: &dyn Db,
    key: AstNodeKey,
    context: CompletionContext,
) -> SemanticQueryResult<Arc<[CompletionCandidate]>> {
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let Some(file) = db.file_registry().lock().expect("file registry").get(key.unit.path(db)).copied() else {
        return Ok(None);
    };
    let source = file.text(db);
    if context.cursor > source.len()
        || context.replacement_start > context.replacement_end
        || context.replacement_end > source.len()
        || !source.is_char_boundary(context.cursor)
        || !source.is_char_boundary(context.replacement_start)
        || !source.is_char_boundary(context.replacement_end)
    {
        return Ok(None);
    }
    let prefix = &source[context.replacement_start..context.replacement_end];
    let before = &source[..context.replacement_start];
    let mut candidates = Vec::new();
    if let Some(before_dot) = before.strip_suffix('.') {
        let alias = before_dot.rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_').next().unwrap_or_default();
        let import_target = {
            let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
            registry
                .imports
                .get(&(key.unit, key.generation))
                .and_then(|imports| imports.iter().find(|import| import.binding == alias))
                .map(|import| import.target)
        };
        if let Some(target) = import_target {
            let Some(target_syntax) = db.syntax_unit(target) else {
                return Ok(None);
            };
            if target_syntax.generation(db) != key.generation {
                return Ok(None);
            }
            let program = target_syntax.expanded_program(db);
            let index = target_syntax.syntax_index(db);
            push_unit_member_candidates(&mut candidates, program, index, prefix, &context);
        } else {
            let program = syntax.expanded_program(db);
            let index = syntax.syntax_index(db);
            let reference = deepest_node_containing_offset(index, context.cursor).unwrap_or(key.node);
            let lookup = AstNodeKey { node: reference, ..key };
            let Some((declaration, _)) = nominal_local_receiver_declaration(db, program, index, lookup, alias) else {
                return Ok(None);
            };
            push_nominal_receiver_candidates(db, &mut candidates, declaration, prefix, &context);
        }
    } else {
        let program = syntax.expanded_program(db);
        let index = syntax.syntax_index(db);
        let reference = deepest_node_containing_offset(index, context.cursor).unwrap_or(key.node);
        push_lexical_local_candidates(&mut candidates, program, index, reference, prefix, &context);
        push_unit_type_candidates(&mut candidates, program, index, prefix, &context, false);
        for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition) {
            if let Some(function) =
                index.node_at(program, id).and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
            {
                push_completion_candidate(
                    &mut candidates,
                    function.name.node.name.as_str(),
                    CompletionKind::Function,
                    None,
                    prefix,
                    &context,
                );
            }
        }
    }
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
    Ok(Some(candidates.into()))
}

/// Capture imported-module member candidates as owned data.
///
/// The returned surface may outlive the Salsa input revision that produced it,
/// allowing completion on a partial current buffer without retaining historical
/// syntax inputs or treating stale syntax as semantic authority.
pub fn completion_dependency_surface(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[CompletionMemberSurface]>> {
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let imports = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .cloned()
        .unwrap_or_default();
    let context = CompletionContext { cursor: 0, replacement_start: 0, replacement_end: 0 };
    let mut surfaces = Vec::new();
    for import in imports {
        let Some(target_syntax) = db.syntax_unit(import.target) else {
            continue;
        };
        if target_syntax.generation(db) != key.generation {
            continue;
        }
        let mut candidates = Vec::new();
        push_unit_member_candidates(
            &mut candidates,
            target_syntax.expanded_program(db),
            target_syntax.syntax_index(db),
            "",
            &context,
        );
        candidates.sort_by(|left, right| left.label.cmp(&right.label));
        candidates.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
        surfaces.push(CompletionMemberSurface {
            receiver: Arc::from(import.binding),
            import_path: import.path.into_iter().map(Arc::from).collect(),
            candidates: candidates.into(),
        });
    }
    surfaces.sort_by(|left, right| left.receiver.cmp(&right.receiver));
    Ok(Some(surfaces.into()))
}

/// Capture imported-module completion candidates directly from a prepared syntax assembly.
///
/// This path deliberately does not require a registered [`AstNodeKey`]. Editors can therefore
/// retain a bounded, owned dependency surface when the current entry buffer is syntactically
/// recoverable but cannot yet produce a typed program.
pub fn completion_dependency_surface_for_assembly(
    assembly: &beskid_analysis::projects::ProgramAssembly,
) -> Arc<[CompletionMemberSurface]> {
    let Some(entry) = assembly.units.get(assembly.entry_index) else {
        return Arc::from([]);
    };
    completion_dependency_surface_for_program(assembly, &entry.program)
}

/// Capture imported-module completion candidates for an editor buffer which may
/// not be the configured build target's entry unit.
pub fn completion_dependency_surface_for_program(
    assembly: &beskid_analysis::projects::ProgramAssembly,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Arc<[CompletionMemberSurface]> {
    let context = CompletionContext { cursor: 0, replacement_start: 0, replacement_end: 0 };
    let mut surfaces = Vec::new();
    for item in &program.node.items {
        let beskid_analysis::syntax::Node::UseDeclaration(declaration) = &item.node else {
            continue;
        };
        let import_path = declaration
            .node
            .path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect::<Vec<_>>();
        let Some(target_path) = (1..=import_path.len())
            .rev()
            .find_map(|length| assembly.module_index.resolve_module(&import_path[..length]))
        else {
            continue;
        };
        let Some((target, index)) = assembly
            .units
            .iter()
            .zip(assembly.syntax_indexes.iter())
            .find(|(unit, _)| beskid_analysis::paths::same_file(&unit.path, target_path))
        else {
            continue;
        };
        let receiver = declaration
            .node
            .alias
            .as_ref()
            .map(|alias| alias.node.name.clone())
            .or_else(|| import_path.last().cloned())
            .unwrap_or_default();
        if receiver.is_empty() {
            continue;
        }
        let mut candidates = Vec::new();
        push_unit_member_candidates(&mut candidates, &target.program, index, "", &context);
        candidates.sort_by(|left, right| left.label.cmp(&right.label));
        candidates.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
        surfaces.push(CompletionMemberSurface {
            receiver: Arc::from(receiver),
            import_path: import_path.into_iter().map(Arc::from).collect(),
            candidates: candidates.into(),
        });
    }
    surfaces.sort_by(|left, right| left.receiver.cmp(&right.receiver));
    surfaces.dedup_by(|left, right| {
        left.receiver == right.receiver && left.import_path == right.import_path && left.candidates == right.candidates
    });
    surfaces.into()
}

pub(super) fn push_completion_candidate(
    candidates: &mut Vec<CompletionCandidate>,
    label: &str,
    kind: CompletionKind,
    detail: Option<&str>,
    prefix: &str,
    context: &CompletionContext,
) {
    if !label.starts_with(prefix) {
        return;
    }
    candidates.push(CompletionCandidate {
        label: Arc::from(label),
        kind,
        detail: detail.map(Arc::from),
        replacement_start: context.replacement_start,
        replacement_end: context.replacement_end,
    });
}

pub(super) fn deepest_node_containing_offset(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    offset: usize,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut best: Option<(usize, beskid_analysis::syntax::AstNodeId)> = None;
    for metadata in index.metadata() {
        let Some(span) = metadata.span else {
            continue;
        };
        if offset < span.start || offset > span.end {
            continue;
        }
        let length = span.end.saturating_sub(span.start);
        if best.is_none_or(|(best_length, best_id)| {
            length < best_length || (length == best_length && metadata.id.0 > best_id.0)
        }) {
            best = Some((length, metadata.id));
        }
    }
    best.map(|(_, id)| id)
}

pub(super) fn push_lexical_local_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    prefix: &str,
    context: &CompletionContext,
) {
    for declaration in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier) {
        if local_declaration_scope(index, declaration, reference).is_none() {
            continue;
        }
        let Some(identifier) =
            index.node_at(program, declaration).and_then(|node| node.of::<beskid_analysis::syntax::Identifier>())
        else {
            continue;
        };
        push_completion_candidate(
            candidates,
            identifier.name.as_str(),
            CompletionKind::Variable,
            Some("local"),
            prefix,
            context,
        );
    }
}

pub(super) fn push_unit_type_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    prefix: &str,
    context: &CompletionContext,
    public_only: bool,
) {
    for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::TypeDefinition) {
        if let Some(definition) =
            index.node_at(program, id).and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
            && (!public_only || definition.visibility.node == beskid_analysis::syntax::Visibility::Public)
        {
            push_completion_candidate(
                candidates,
                definition.name.node.name.as_str(),
                CompletionKind::Type,
                Some("type"),
                prefix,
                context,
            );
        }
    }
    for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::EnumDefinition) {
        if let Some(definition) =
            index.node_at(program, id).and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
            && (!public_only || definition.visibility.node == beskid_analysis::syntax::Visibility::Public)
        {
            push_completion_candidate(
                candidates,
                definition.name.node.name.as_str(),
                CompletionKind::Type,
                Some("enum"),
                prefix,
                context,
            );
        }
    }
}

pub(super) fn push_unit_member_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    prefix: &str,
    context: &CompletionContext,
) {
    for id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition) {
        if let Some(function) =
            index.node_at(program, id).and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
            && function.visibility.node == beskid_analysis::syntax::Visibility::Public
        {
            push_completion_candidate(
                candidates,
                function.name.node.name.as_str(),
                CompletionKind::Function,
                None,
                prefix,
                context,
            );
        }
    }
    push_unit_type_candidates(candidates, program, index, prefix, context, true);
}

pub(super) fn push_nominal_receiver_candidates(
    db: &dyn Db,
    candidates: &mut Vec<CompletionCandidate>,
    declaration: AstNodeKey,
    prefix: &str,
    context: &CompletionContext,
) {
    let Some(declaration_syntax) = db.syntax_unit(declaration.unit) else {
        return;
    };
    if declaration_syntax.generation(db) != declaration.generation {
        return;
    }
    let program = declaration_syntax.expanded_program(db);
    let index = declaration_syntax.syntax_index(db);
    let Some(definition) =
        index.node_at(program, declaration.node).and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
    else {
        return;
    };
    for field in &definition.fields {
        push_completion_candidate(
            candidates,
            field.node.name.node.name.as_str(),
            CompletionKind::Field,
            Some("field"),
            prefix,
            context,
        );
    }
    for method in &definition.methods {
        push_completion_candidate(
            candidates,
            method.node.name.node.name.as_str(),
            CompletionKind::Method,
            Some("method"),
            prefix,
            context,
        );
    }
    if let Some(children) = index.children(declaration.node) {
        for child in children.iter().copied() {
            if let Some(method) =
                index.node_at(program, child).and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
            {
                push_completion_candidate(
                    candidates,
                    method.name.node.name.as_str(),
                    CompletionKind::Method,
                    Some("method"),
                    prefix,
                    context,
                );
            }
        }
    }
}
