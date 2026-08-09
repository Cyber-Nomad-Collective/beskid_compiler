//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked]
pub(super) fn resolved_item_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ResolvedItem> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        resolve_item_declaration(db, program, index, key, &path.path.node)
            .map(|declaration| ResolvedItem { declaration })
    })
}

pub(super) fn resolve_item_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    // A generic receiver remains an exact module/type namespace fact: `Channel<i64>.Create`
    // names the `Create` item imported from `Concurrency.Channel`.  Only a generic terminal
    // callee would require unimplemented function monomorphization.
    if path.segments.last().is_some_and(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    resolve_item_declaration_candidate(db, program, index, key, path)
}

/// Resolve a function declaration without accepting terminal generic syntax as a call fact.
/// Callers must validate an explicit terminal instantiation through
/// [`generic_call_instantiation`] before treating this candidate as callable.
pub(super) fn resolve_item_declaration_candidate(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (name, module_path) = path.segments.split_last()?;
    if module_path.is_empty() {
        let name = name.node.name.node.name.as_str();
        if resolve_lexical_declaration(program, index, key.node, name).is_some() {
            return None;
        }
        return resolve_unqualified_item_declaration(program, index, key, name)
            .or_else(|| unique_function_in_unit(db, key.unit, key.generation, name))
            .or_else(|| unique_imported_function(db, key, name));
    }
    let module_path = module_path.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    let Some(target_unit) = resolve_qualified_module_unit(db, key, &module_path) else {
        if path.segments[..path.segments.len() - 1].iter().all(|segment| segment.node.type_args.is_empty())
            && let Some(declaration) = resolve_inline_module_item_declaration(program, index, key, path)
        {
            return Some(declaration);
        }
        return resolve_type_qualified_imported_function(db, key, path);
    };
    unique_exported_function_in_unit(db, target_unit, key.generation, &name.node.name.node.name)
}

/// Resolve a qualified function below a lexical inline-module path in the current syntax unit.
///
/// Inline modules do not have an assembly `SourceUnitId`, so they cannot appear in the dependency
/// registry used for imported module resolution.  Their namespace is nevertheless fully indexed
/// in the current `SyntaxUnitInput`: walk exact direct `InlineModule` children from the nearest
/// lexical module scope outward, then select one exact function in the resulting scope.  Ambiguous
/// paths remain unavailable; this is deliberately not a name-based dynamic fallback.
pub(super) fn resolve_inline_module_item_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (terminal, module_path) = path.segments.split_last()?;
    if module_path.is_empty() || !terminal.node.type_args.is_empty() {
        return None;
    }
    let module_names = module_path.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>();
    let function_name = terminal.node.name.node.name.as_str();

    let mut scope = module_scope(index, key.node)?;
    loop {
        let mut current = scope;
        let mut path_exists = true;
        for module_name in &module_names {
            let Some(module) = unique_inline_module_in_scope(program, index, current, module_name) else {
                path_exists = false;
                break;
            };
            current = module;
        }
        if !path_exists {
            scope = outer_module_scope(index, scope)?;
            continue;
        }
        let functions = index
            .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
            .filter(|candidate| {
                module_scope(index, *candidate) == Some(current)
                    && index
                        .node_at(program, *candidate)
                        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                        .is_some_and(|function| function.name.node.name == function_name)
            })
            .collect::<Vec<_>>();
        if let [declaration] = functions.as_slice() {
            return Some(AstNodeKey { node: *declaration, ..key });
        }
        // An existing lexical module with a missing or ambiguous terminal must not fall through
        // to an outer namespace or import route and silently select a different callable.
        return None;
    }
}

pub(super) fn unique_inline_module_in_scope(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    scope: beskid_analysis::syntax::AstNodeId,
    name: &str,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let modules = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::InlineModule)
        .filter(|candidate| {
            // `module_scope(candidate)` returns the inline module itself.  Its declaration
            // belongs to the enclosing scope, so start the ancestor lookup at its parent.
            outer_module_scope(index, *candidate) == Some(scope)
                && index
                    .node_at(program, *candidate)
                    .and_then(|node| node.of::<beskid_analysis::syntax::InlineModule>())
                    .is_some_and(|module| module.name.node.name == name)
        })
        .collect::<Vec<_>>();
    let [module] = modules.as_slice() else {
        return None;
    };
    Some(*module)
}

/// Resolve a qualified module path from an exact current import and, when required, explicit
/// public child-module edges. Private imports remain available only inside their owner.
pub(super) fn resolve_qualified_module_unit(
    db: &dyn Db,
    key: AstNodeKey,
    module_path: &[String],
) -> Option<SourceUnitId> {
    let initial = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))?
        .iter()
        .filter_map(|import| import_path_prefix_len(import, module_path).map(|consumed| (import.target, consumed)))
        .collect::<Vec<_>>();

    let mut resolved = Vec::new();
    for (unit, consumed) in initial {
        let mut pending = vec![(unit, consumed)];
        let mut visited = std::collections::HashSet::new();
        while let Some((current, consumed)) = pending.pop() {
            if !visited.insert((current, consumed)) {
                continue;
            }
            if consumed == module_path.len() {
                if !resolved.contains(&current) {
                    resolved.push(current);
                }
                continue;
            }
            let segment = &module_path[consumed];
            pending.extend(
                public_module_routes(db, current, key.generation)
                    .into_iter()
                    .filter_map(|(binding, child)| (binding == *segment).then_some((child, consumed + 1))),
            );
        }
    }
    let [unit] = resolved.as_slice() else {
        return None;
    };
    Some(*unit)
}

pub(super) fn import_path_prefix_len(import: &crate::db::SyntaxImport, module_path: &[String]) -> Option<usize> {
    // A source unit owns only the names bound by its own `use` declarations. Original import
    // paths may be used only for an unaliased import, where the terminal path segment is that
    // binding, and only for the target module itself. Registry suffixes or child routes would
    // bypass aliases and make visibility order-dependent.
    module_path.first().filter(|segment| import.binding == **segment).map(|_| 1).or_else(|| {
        (!import.has_explicit_alias && import.binding == *import.path.last()? && module_path == import.path.as_slice())
            .then_some(import.path.len())
    })
}

/// Return public routes with their bindings: a target may be re-exported under multiple aliases.
pub(super) fn public_module_routes(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
) -> Vec<(String, SourceUnitId)> {
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(unit, generation))
        .into_iter()
        .flatten()
        .filter(|import| import.public)
        .map(|import| (import.binding.clone(), import.target))
        .collect()
}

/// Resolve `Imported.ModuleType.Function()` only when the import identifies one source unit,
/// the extra qualifier identifies one declared type in that unit, and the terminal function is
/// likewise unique. This preserves a direct call edge for the Corelib convention of spelling
/// static functions through their nominal type without treating arbitrary nested paths as
/// callable.
pub(super) fn resolve_type_qualified_imported_function(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (function, module_path) = path.segments.split_last()?;
    let (type_segment, import_path) = module_path.split_last()?;
    let import_path = import_path.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    let target_unit = resolve_qualified_module_unit(db, key, &import_path)?;
    unique_exported_type_in_unit(
        db,
        target_unit,
        key.generation,
        &type_segment.node.name.node.name,
        type_segment.node.type_args.len(),
    )?;
    unique_exported_function_in_unit(db, target_unit, key.generation, &function.node.name.node.name)
}

/// Resolve a public module member through its defining syntax unit or an explicit public
/// re-export. This is intentionally limited to assembly-registered `pub use` edges, so a
/// private implementation import cannot become visible through its parent module.
///
/// Re-export distance decides precedence: a declaration in the named unit shadows the same name
/// reached through that unit's public re-exports. Hub modules define a flat helper *and*
/// re-export the child module the helper delegates to (`Core.String.Len` next to
/// `pub mod Core.String.Core`), so collecting every route as a peer would make the hub's own
/// surface permanently ambiguous. Ambiguity between routes at the same distance still fails
/// closed.
pub(super) fn unique_exported_function_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> Option<AstNodeKey> {
    nearest_reexport_route(db, unit, generation, |db, current, generation| {
        unique_public_function_in_unit(db, current, generation, name)
    })
}

/// Walk public re-export edges breadth-first and return the single candidate at the shallowest
/// distance that yields one. More than one candidate at that distance is a genuine ambiguity and
/// resolves to nothing.
pub(super) fn nearest_reexport_route(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    candidate_in_unit: impl Fn(&dyn Db, SourceUnitId, SyntaxGenerationId) -> Option<AstNodeKey>,
) -> Option<AstNodeKey> {
    let mut frontier = vec![unit];
    let mut visited = std::collections::HashSet::new();
    visited.insert(unit);
    while !frontier.is_empty() {
        let mut candidates: Vec<AstNodeKey> = Vec::new();
        let mut next = Vec::new();
        for current in frontier {
            if let Some(candidate) = candidate_in_unit(db, current, generation)
                && !candidates.contains(&candidate)
            {
                candidates.push(candidate);
            }
            next.extend(
                public_reexport_units(db, current, generation).into_iter().filter(|target| visited.insert(*target)),
            );
        }
        match candidates.as_slice() {
            [candidate] => return Some(*candidate),
            [] => frontier = next,
            _ => return None,
        }
    }
    None
}

pub(super) fn public_reexport_units(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
) -> Vec<SourceUnitId> {
    public_module_routes(db, unit, generation).into_iter().map(|(_, target)| target).fold(
        Vec::new(),
        |mut targets, target| {
            if !targets.contains(&target) {
                targets.push(target);
            }
            targets
        },
    )
}

pub(super) fn unique_public_function_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let candidates = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
        .filter(|candidate| {
            index
                .node_at(program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                .is_some_and(|function| {
                    function.visibility.node == beskid_analysis::syntax::Visibility::Public
                        && function.name.node.name == name
                })
        })
        .collect::<Vec<_>>();
    let [node] = candidates.as_slice() else {
        return None;
    };
    Some(AstNodeKey { unit, generation, node: *node })
}

/// Resolve an exact function name only when the syntax unit has one unambiguous definition.
/// This preserves reachability for macro-expanded items whose synthetic nodes no longer retain
/// their original module ancestry.
pub(super) fn unique_function_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let candidates = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
        .filter(|candidate| {
            index
                .node_at(program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                .is_some_and(|function| function.name.node.name == name)
        })
        .collect::<Vec<_>>();
    let [node] = candidates.as_slice() else {
        return None;
    };
    Some(AstNodeKey { unit, generation, node: *node })
}

/// Resolve an unqualified imported function only when its assembled import targets provide one
/// exact declaration. Arbitrary unresolved bare names deliberately remain unavailable.
pub(super) fn unique_imported_function(db: &dyn Db, key: AstNodeKey, name: &str) -> Option<AstNodeKey> {
    let targets = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))?
        .iter()
        .map(|import| import.target)
        .fold(Vec::new(), |mut targets, target| {
            if !targets.contains(&target) {
                targets.push(target);
            }
            targets
        });
    let candidates = targets
        .into_iter()
        .filter_map(|target| unique_exported_function_in_unit(db, target, key.generation, name))
        .collect::<Vec<_>>();
    let [declaration] = candidates.as_slice() else {
        return None;
    };
    Some(*declaration)
}

pub(super) fn resolve_unqualified_item_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    name: &str,
) -> Option<AstNodeKey> {
    if resolve_lexical_declaration(program, index, key.node, name).is_some() {
        return None;
    }

    let mut scope = module_scope(index, key.node)?;
    loop {
        let candidates = index
            .ids_of_kind(beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
            .filter(|candidate| {
                module_scope(index, *candidate) == Some(scope)
                    && index
                        .node_at(program, *candidate)
                        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                        .is_some_and(|function| function.name.node.name == name)
            })
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [declaration] => {
                return Some(AstNodeKey { node: *declaration, ..key });
            }
            [] => {}
            _ => return None,
        }
        scope = outer_module_scope(index, scope)?;
    }
}

pub(super) fn module_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    nearest_ancestor(index, node, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::InlineModule | beskid_analysis::syntax_query::NodeKind::Program
        )
    })
}

pub(super) fn outer_module_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    scope: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    module_scope(index, parent_node(index, scope)?)
}
