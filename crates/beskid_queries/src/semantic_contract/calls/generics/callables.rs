//! Generic callable parameters, imported receivers, and generic call instantiation.

use super::super::super::*;
use super::*;

/// A two-segment imported nominal call can spell either a module member or a static member on a
/// generic nominal type.  The latter has no concrete receiver ABI until the source supplies the
/// receiver arguments (`Hub<i64>.Create()`), so it must not be treated as the imported module's
/// direct function.  Terminal method arguments remain independently valid (`Hub.Create<i64>()`).
pub(in crate::semantic_contract) fn imported_generic_nominal_receiver_requires_instantiation(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let [receiver, method] = path.segments.as_slice() else {
        return false;
    };
    if !receiver.node.type_args.is_empty() || !method.node.type_args.is_empty() {
        return false;
    }
    let receiver_name = receiver.node.name.node.name.as_str();
    let targets = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .into_iter()
        .flatten()
        .filter(|import| import.binding == receiver_name)
        .map(|import| import.target)
        .collect::<Vec<_>>();
    let [target] = targets.as_slice() else {
        return false;
    };
    exported_generic_type_named(db, *target, key.generation, receiver_name)
}

pub(in crate::semantic_contract) fn exported_generic_type_named(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> bool {
    let mut pending = vec![unit];
    let mut visited = std::collections::HashSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        let Some(syntax) = db.syntax_unit(current) else {
            continue;
        };
        if syntax.generation(db) != generation {
            continue;
        }
        if syntax.syntax_index(db).ids_of_kind(beskid_analysis::syntax_query::NodeKind::TypeDefinition).any(
            |candidate| {
                syntax
                    .syntax_index(db)
                    .node_at(syntax.expanded_program(db), candidate)
                    .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                    .is_some_and(|definition| definition.name.node.name == name && !definition.generics.is_empty())
            },
        ) {
            return true;
        }
        pending.extend(public_reexport_units(db, current, generation));
    }
    false
}

pub(in crate::semantic_contract) fn generic_call_instantiation_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<GenericCallInstantiation> {
    let argument_syntax = explicit_generic_type_argument_syntax(path)?;
    let argument_count = u8::try_from(argument_syntax.len()).ok()?;
    (argument_count > 0).then_some(())?;
    let declaration = resolve_item_declaration_candidate(db, program, index, key, path)?;
    let syntax = db.syntax_unit(declaration.unit)?;
    syntax.accepts_key(db, declaration).then_some(())?;
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<beskid_analysis::syntax::FunctionDefinition>()?;
    (function.generics.len() == usize::from(argument_count)).then_some(())?;
    let mut concrete_arguments = Vec::with_capacity(argument_syntax.len());
    for (argument, generic) in argument_syntax.iter().zip(function.generics.iter()) {
        match abi_type_from_syntax(db, key, &argument.node) {
            Ok(concrete) => concrete_arguments.push(concrete),
            Err(_) if type_syntax_is_generic_parameter_reference(&argument.node, generic.node.name.as_str()) => {}
            Err(_) => return None,
        }
    }
    Some(GenericCallInstantiation { declaration, argument_count, arguments: concrete_arguments.into() })
}

pub(in crate::semantic_contract) fn function_declares_generics(db: &dyn Db, declaration: AstNodeKey) -> bool {
    matches!(generic_callable_parameters(db, declaration), Some((parameters, false)) if !parameters.is_empty())
}

/// Return declaration-ordered generic names and whether the callable has an implicit receiver.
/// Generic methods inherit their parameters from exactly one owning generic type definition.
pub(in crate::semantic_contract) fn generic_callable_parameters(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<(Vec<&str>, bool)> {
    let syntax = db.syntax_unit(declaration.unit)?;
    if !syntax.accepts_key(db, declaration) {
        return None;
    }
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return (!function.generics.is_empty())
            .then(|| (function.generics.iter().map(|generic| generic.node.name.as_str()).collect(), false));
    }
    node.of::<beskid_analysis::syntax::MethodDefinition>()?;
    let owner = method_owner_node(syntax.expanded_program(db), syntax.syntax_index(db), declaration.node)
        .and_then(|parent| syntax.syntax_index(db).node_at(syntax.expanded_program(db), parent))
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())?;
    (!owner.generics.is_empty())
        .then(|| (owner.generics.iter().map(|generic| generic.node.name.as_str()).collect(), true))
}

/// Whether a qualified call's receiver is an exact current import target.
/// Imported type/module member calls have no direct item edge; unknown qualified calls remain
/// unavailable instead of being guessed.
pub(in crate::semantic_contract) fn imported_call_receiver_exists(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some((_member, receiver)) = path.segments.split_last() else {
        return false;
    };
    if receiver.is_empty() {
        return false;
    }
    let receiver = receiver.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>();
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .is_some_and(|imports| {
            imports
                .iter()
                .filter(|import| {
                    (receiver.len() == 1 && import.binding == receiver[0])
                        || (import.path.len() >= receiver.len()
                            && import.path[import.path.len() - receiver.len()..]
                                .iter()
                                .map(String::as_str)
                                .eq(receiver.iter().copied()))
                })
                .take(2)
                .count()
                == 1
        })
}

pub(in crate::semantic_contract) fn expression_is_lambda(expression: &beskid_analysis::syntax::Expression) -> bool {
    match expression {
        beskid_analysis::syntax::Expression::Lambda(_) => true,
        beskid_analysis::syntax::Expression::Grouped(grouped) => expression_is_lambda(&grouped.node.expr.node),
        _ => false,
    }
}
