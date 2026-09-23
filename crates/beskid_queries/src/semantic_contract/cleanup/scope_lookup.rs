//! Visible units, cleanup contract declarations, and cleanup type lookup.

use super::super::*;
use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractNode, FunctionDefinition, MethodDefinition, PrimitiveType, ScopedUseStatement, Type,
    TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

fn cleanup_visible_units(db: &dyn Db, key: AstNodeKey) -> Vec<SourceUnitId> {
    let mut units = vec![key.unit];
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    for import in registry.imports.get(&(key.unit, key.generation)).into_iter().flatten() {
        if !units.contains(&import.target) {
            units.push(import.target);
        }
    }
    units
}

pub(super) fn cleanup_contract_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (name, modules) = path.segments.split_last()?;
    if name.node.name.node.name != "Disposable" || !name.node.type_args.is_empty() {
        return None;
    }
    let units = if modules.is_empty() {
        cleanup_visible_units(db, key)
    } else {
        vec![resolve_qualified_module_unit(
            db,
            key,
            &modules.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>(),
        )?]
    };
    let mut candidates = Vec::new();
    for unit in units {
        let syntax = db.syntax_unit(unit)?;
        if syntax.generation(db) != key.generation {
            return None;
        }
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::ContractDefinition) {
            if !cleanup_declaration_in_scope(db, key, AstNodeKey { unit, node, ..key }) {
                continue;
            }
            let definition =
                syntax.syntax_index(db).node_at(syntax.expanded_program(db), node)?.of::<ContractDefinition>()?;
            if definition.name.node.name == "Disposable"
                && (unit == key.unit || definition.visibility.node == Visibility::Public)
            {
                candidates.push(AstNodeKey { unit, node, ..key });
            }
        }
    }
    match candidates.as_slice() {
        [candidate] => Some(*candidate),
        _ => None,
    }
}

fn cleanup_declaration_in_scope(db: &dyn Db, use_site: AstNodeKey, declaration: AstNodeKey) -> bool {
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    let index = syntax.syntax_index(db);
    let scope = module_scope(index, declaration.node);
    if declaration.unit == use_site.unit {
        scope == module_scope(index, use_site.node)
    } else {
        scope.is_some_and(|scope| index.kind(scope) == Some(NodeKind::Program))
    }
}

pub(super) fn cleanup_named_type(db: &dyn Db, key: AstNodeKey, ty: &Type, name: &str) -> Option<AstNodeKey> {
    let Type::Complex(path) = ty else {
        return None;
    };
    (path.node.segments.last()?.node.name.node.name == name).then_some(())?;
    resolve_type_declaration(db, key, &path.node)
}

pub(super) fn same_cleanup_type(
    db: &dyn Db,
    left_key: AstNodeKey,
    left: &Type,
    right_key: AstNodeKey,
    right: &Type,
) -> bool {
    match (left, right) {
        (Type::Complex(left), Type::Complex(right)) => {
            let left_definition = resolve_type_declaration(db, left_key, &left.node);
            let right_definition = resolve_type_declaration(db, right_key, &right.node);
            left_definition.is_some()
                && left_definition == right_definition
                && left.node.segments.last().zip(right.node.segments.last()).is_some_and(|(left, right)| {
                    left.node.type_args.len() == right.node.type_args.len()
                        && left
                            .node
                            .type_args
                            .iter()
                            .zip(&right.node.type_args)
                            .all(|(left, right)| same_cleanup_type(db, left_key, &left.node, right_key, &right.node))
                })
        }
        (Type::Primitive(left), Type::Primitive(right)) => left.node == right.node,
        (Type::Array(left), Type::Array(right)) => same_cleanup_type(db, left_key, &left.node, right_key, &right.node),
        _ => false,
    }
}

pub(super) fn cleanup_conversion_candidates(
    db: &dyn Db,
    key: AstNodeKey,
    error_key: AstNodeKey,
    dispose_error: &Type,
    enclosing_error: &Type,
) -> (Vec<AstNodeKey>, bool) {
    let mut candidates = Vec::new();
    let mut invalid = false;
    for unit in cleanup_visible_units(db, key) {
        let Some(syntax) = db.syntax_unit(unit) else {
            invalid = true;
            continue;
        };
        if syntax.generation(db) != key.generation {
            invalid = true;
            continue;
        }
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
            if !cleanup_declaration_in_scope(db, key, AstNodeKey { unit, node, ..key }) {
                continue;
            }
            let Some(function) = syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), node)
                .and_then(|node| node.of::<FunctionDefinition>())
            else {
                continue;
            };
            if !function.attributes.iter().any(|attribute| attribute.node.name.node.name == "CleanupConversion")
                || (unit != key.unit && function.visibility.node != Visibility::Public)
            {
                continue;
            }
            let candidate = AstNodeKey { unit, node, ..key };
            let eligible = function.generics.is_empty()
                && function.parameters.len() == 1
                && same_cleanup_type(db, candidate, &function.parameters[0].node.ty.node, error_key, dispose_error)
                && function
                    .return_type
                    .as_ref()
                    .is_some_and(|result| same_cleanup_type(db, candidate, &result.node, key, enclosing_error));
            if eligible {
                candidates.push(candidate);
            } else {
                invalid = true;
            }
        }
    }
    (candidates, invalid)
}
