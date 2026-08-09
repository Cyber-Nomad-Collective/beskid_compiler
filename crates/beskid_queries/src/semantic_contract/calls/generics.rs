//! Focused call-semantics implementation.

use super::super::*;

#[salsa::tracked]
pub(in crate::semantic_contract) fn generic_call_instantiation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallInstantiation> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        generic_call_instantiation_for_node(db, program, index, key, &path.node.path.node).map(Ok)
    })?
    .transpose()
}
#[salsa::tracked]
pub(in crate::semantic_contract) fn generic_call_specialization_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallSpecialization> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::CallExpression>()?;
        let lowering = match call_lowering(db, key) {
            Ok(Some(lowering)) => lowering,
            Ok(None) => return None,
            // Unavailable call sites cannot contribute call-derived ABI specializations.
            // Propagating the error aborted whole-module emission for Core.Output (enum
            // constructors / unresolved paths in the reachable Syscall body).
            Err(error) if error.is_unavailable() => return None,
            Err(error) => return Some(Err(error)),
        };
        let declaration = match lowering {
            CallLowering::Direct(declaration) => declaration,
            CallLowering::Dynamic | CallLowering::Runtime(_) | CallLowering::CorelibService(_) => {
                return None;
            }
        };
        let declaration_syntax = db.syntax_unit(declaration.unit)?;
        let declaration_node =
            declaration_syntax.syntax_index(db).node_at(declaration_syntax.expanded_program(db), declaration.node)?;
        let function = declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>()?;
        if function.generics.is_empty() {
            return None;
        }
        let instance = match generic_specialization_instance_for_call(db, key) {
            Ok(instance) => instance,
            Err(error) => return Some(Err(error)),
        };
        Some(Ok(GenericCallSpecialization {
            declaration: instance.declaration,
            signature: instance.signature,
            substitutions: instance.substitutions,
        }))
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn generic_call_template_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallTemplate> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let argument_syntax = explicit_generic_type_argument_syntax(&path.node.path.node)?;
        let declaration = resolve_item_declaration_candidate(db, program, index, key, &path.node.path.node)?;
        let declaration_syntax = db.syntax_unit(declaration.unit)?;
        let function = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::FunctionDefinition>()?;
        (function.generics.len() == argument_syntax.len()).then_some(())?;
        let parameter_arguments = argument_syntax
            .iter()
            .map(|argument| generic_parameter_reference_name(&argument.node).map(Arc::<str>::from))
            .collect::<Option<Vec<_>>>()?;
        let enclosing = nearest_ancestor(index, key.node, |kind| {
            kind == beskid_analysis::syntax_query::NodeKind::FunctionDefinition
        })?;
        let enclosing = index.node_at(program, enclosing)?.of::<beskid_analysis::syntax::FunctionDefinition>()?;
        if !parameter_arguments
            .iter()
            .all(|argument| enclosing.generics.iter().any(|generic| generic.node.name.as_str() == argument.as_ref()))
        {
            return None;
        }
        let parameters =
            function.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect::<Vec<_>>();
        Some(Ok(GenericCallTemplate {
            declaration,
            parameters: parameters.into(),
            parameter_arguments: parameter_arguments.into(),
        }))
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn explicit_generic_type_argument_syntax(
    path: &beskid_analysis::syntax::Path,
) -> Option<&[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>]> {
    let terminal = path.segments.last()?;
    let receiver = path.segments.get(..path.segments.len().checked_sub(1)?)?;
    let receiver_with_arguments =
        receiver.iter().filter(|segment| !segment.node.type_args.is_empty()).collect::<Vec<_>>();
    let terminal_has_arguments = !terminal.node.type_args.is_empty();
    match (terminal_has_arguments, receiver_with_arguments.as_slice()) {
        (true, []) => Some(terminal.node.type_args.as_slice()),
        (false, [receiver]) => Some(receiver.node.type_args.as_slice()),
        _ => None,
    }
}

pub(in crate::semantic_contract) fn type_syntax_is_generic_parameter_reference(
    syntax_type: &beskid_analysis::syntax::Type,
    parameter_name: &str,
) -> bool {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return false;
    };
    let [segment] = path.node.segments.as_slice() else {
        return false;
    };
    segment.node.type_args.is_empty() && segment.node.name.node.name == parameter_name
}

fn type_syntax_is_enclosing_generic_parameter_reference(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> bool {
    let Some(parameter_name) = generic_parameter_reference_name(syntax_type) else {
        return false;
    };
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, key) {
        return false;
    }
    let index = syntax.syntax_index(db);
    let Some(enclosing) =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
    else {
        return false;
    };
    index
        .node_at(syntax.expanded_program(db), enclosing)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .is_some_and(|function| function.generics.iter().any(|generic| generic.node.name == parameter_name.as_ref()))
}

pub(in crate::semantic_contract) fn generic_call_uses_parameter_type_arguments(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some(type_arguments) = explicit_generic_type_argument_syntax(path) else {
        return false;
    };
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, declaration) {
        return false;
    }
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
    else {
        return false;
    };
    if function.generics.len() != type_arguments.len() {
        return false;
    }
    type_arguments.iter().zip(function.generics.iter()).all(|(argument, generic)| {
        abi_type_from_syntax(db, key, &argument.node).is_ok()
            || type_syntax_is_generic_parameter_reference(&argument.node, generic.node.name.as_str())
            || type_syntax_is_enclosing_generic_parameter_reference(db, key, &argument.node)
    })
}

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
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, declaration) {
        return false;
    }
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .is_some_and(|function| !function.generics.is_empty())
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
