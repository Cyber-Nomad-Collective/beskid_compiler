//! Calls through function-typed local values.
//!
//! A function value is a reference to a GC-managed closure record. Calls whose callee is a
//! statically known item or an inline lambda keep their own authorities; this fact covers only a
//! single-segment callee naming a function-typed parameter or local, whose record is read at the
//! call site and invoked indirectly.

use super::super::abi::declared_callable_return_type;
use super::super::layouts::explicit_local_declaration_type;
use super::super::typing::managed_reference_kind_for_syntax_type;
use super::super::*;
use super::*;

/// How one function-typed local declaration proves its call signature.
enum FunctionValueType<'a> {
    /// A declared function type on a parameter, typed lambda parameter, or annotated local.
    Declared(&'a beskid_analysis::syntax::Type),
    /// An unannotated local initialized by this lambda expression node.
    Lambda(beskid_analysis::syntax::AstNodeId),
}

/// Return the function-typed lexical declaration named by a single-segment call callee.
pub(in crate::semantic_contract) fn function_value_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    path: &beskid_analysis::syntax::Path,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let [segment] = path.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, reference, segment.node.name.node.name.as_str())?;
    function_value_type(program, index, declaration).map(|_| declaration)
}

fn function_value_type<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<FunctionValueType<'a>> {
    let parent = parent_node(index, declaration)?;
    let parent_syntax = index.node_at(program, parent)?;
    let declared = explicit_local_declaration_type(program, index, declaration).or_else(|| {
        parent_syntax
            .of::<beskid_analysis::syntax::LambdaParameter>()
            .and_then(|parameter| parameter.ty.as_ref().map(|ty| &ty.node))
    });
    if let Some(declared) = declared {
        return matches!(declared, beskid_analysis::syntax::Type::Function { .. })
            .then_some(FunctionValueType::Declared(declared));
    }
    let binding = parent_syntax.of::<beskid_analysis::syntax::LetStatement>()?;
    if !expression_is_lambda(&binding.value.node) {
        return None;
    }
    let value =
        index.direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(&binding.value))?;
    Some(FunctionValueType::Lambda(normalized_expression_node(index, value)))
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn function_value_call_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<FunctionValueCall> {
    function_value_call_in_environment(db, syntax, key, &[])
}

/// [`function_value_call`] inside one concrete generic specialization of its enclosing function.
///
/// A declared function type that names the enclosing declaration's generic parameters (for
/// example `(T) => T f` in `T Apply<T>(...)`) has no context-free ABI; `substitutions` binds them.
pub fn function_value_call_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    substitutions: &[GenericSubstitution],
) -> SemanticQueryResult<FunctionValueCall> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    function_value_call_in_environment(db, syntax, key, substitutions)
}

fn function_value_call_in_environment(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    substitutions: &[GenericSubstitution],
) -> SemanticQueryResult<FunctionValueCall> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let declaration = function_value_declaration(program, index, key.node, &path.node.path.node)?;
        let callee = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()))
            .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })?;
        let declaration = AstNodeKey { node: declaration, ..key };
        let environment = substitutions
            .iter()
            .map(|substitution| (substitution.parameter.to_string(), substitution.argument))
            .collect::<HashMap<_, _>>();
        let shape = (|| match function_value_type(program, index, declaration.node) {
            Some(FunctionValueType::Declared(beskid_analysis::syntax::Type::Function { return_type, parameters })) => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| generic_abi_type(db, declaration, &parameter.node, &environment))
                    .collect::<Result<Vec<_>, _>>()?;
                let result = generic_abi_type(db, declaration, &return_type.node, &environment)?;
                let generic_result = generic_parameter_reference_name(&return_type.node).and_then(|name| {
                    substitutions.iter().find(|substitution| substitution.parameter.as_ref() == name)
                });
                let managed = match generic_result {
                    Some(substitution) => substitution.managed_reference_kind(),
                    None => managed_reference_kind_for_syntax_type(&return_type.node)?,
                };
                Ok((ItemSignature { parameters: parameters.into(), result }, managed))
            }
            Some(FunctionValueType::Lambda(lambda)) => {
                let lambda = AstNodeKey { node: lambda, ..key };
                let signature = closure_signature_tracked(db, syntax, lambda)?
                    .ok_or_else(|| SemanticError::unavailable("function_value_call"))?;
                let managed = if matches!(signature.callable.result, SemanticTypeId::UNIT | SemanticTypeId::NEVER) {
                    ManagedReferenceKind::NativeOrScalar
                } else {
                    managed_reference_kind(db, signature.body)?
                        .ok_or_else(|| SemanticError::unavailable("function_value_call"))?
                };
                Ok((signature.callable, managed))
            }
            _ => Err(SemanticError::unavailable("function_value_call")),
        })();
        Some(shape.map(|(signature, result_managed_reference)| FunctionValueCall {
            call: key,
            callee,
            declaration,
            signature,
            result_managed_reference,
        }))
    })?
    .transpose()
}

/// Decide whether a lambda expression must materialize its closure record.
///
/// Immediate calls, spawn operands, and event-handler locals have their own lowering. A local
/// bound to a lambda needs a record only when it is mutable or when some use of it is not the
/// callee of a call; calls through an immutable lambda local stay inline. Every other lambda
/// position (an argument, a return value, an assigned value) is a function value.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn lambda_value_required_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<bool> {
    let event_handler = event_handler_lambda_for_local_tracked(db, syntax, key)?.is_some();
    with_node(db, syntax, key, |program, index, node| {
        node.of::<beskid_analysis::syntax::LambdaExpression>()?;
        let Some(parent) = value_use_parent(index, key.node) else {
            return Some(true);
        };
        if is_call_callee(program, index, parent, key.node) {
            return Some(false);
        }
        match index.kind(parent) {
            Some(beskid_analysis::syntax_query::NodeKind::SpawnExpression) => Some(false),
            Some(beskid_analysis::syntax_query::NodeKind::LetStatement) => {
                if event_handler {
                    return Some(false);
                }
                let binding = index.node_at(program, parent)?.of::<beskid_analysis::syntax::LetStatement>()?;
                if binding.mutable || lambda_has_statement_body(program, index, key.node) {
                    return Some(true);
                }
                let name = index
                    .direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(&binding.name))?;
                let escapes = index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::PathExpression).any(|path| {
                    let Some(path_syntax) = index
                        .node_at(program, path)
                        .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
                    else {
                        return false;
                    };
                    let [segment] = path_syntax.path.node.segments.as_slice() else {
                        return false;
                    };
                    resolve_lexical_declaration(program, index, path, segment.node.name.node.name.as_str())
                        == Some(name)
                        && !value_use_parent(index, path)
                            .is_some_and(|parent| is_call_callee(program, index, parent, path))
                });
                Some(escapes)
            }
            _ => Some(true),
        }
    })
}

/// The first ancestor of `node` that is not a transparent expression wrapper.
fn value_use_parent(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut current = parent_node(index, node)?;
    while matches!(
        index.kind(current),
        Some(
            beskid_analysis::syntax_query::NodeKind::Expression
                | beskid_analysis::syntax_query::NodeKind::GroupedExpression
        )
    ) {
        current = parent_node(index, current)?;
    }
    Some(current)
}

fn is_call_callee(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    parent: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    index
        .node_at(program, parent)
        .and_then(|parent_syntax| parent_syntax.of::<beskid_analysis::syntax::CallExpression>())
        .and_then(|call| {
            index.direct_child_id(
                program,
                parent,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            )
        })
        .is_some_and(|callee| normalized_expression_node(index, callee) == node)
}

/// ABI signature of the function type a lambda is written against, when its position declares
/// one: an annotated local, the declared return type of the enclosing function, or the parameter
/// of a directly called function. Untyped lambda parameters take its parameter types, and a block
/// body (whose value comes from `return`) takes its result type.
pub(in crate::semantic_contract) fn lambda_expected_signature(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    lambda: AstNodeKey,
) -> Option<Result<ItemSignature, SemanticError>> {
    let parent = value_use_parent(index, lambda.node)?;
    let parent_key = AstNodeKey { node: parent, ..lambda };
    let parent_syntax = index.node_at(program, parent)?;
    if let Some(binding) = parent_syntax.of::<beskid_analysis::syntax::LetStatement>() {
        return function_type_signature(db, parent_key, &binding.type_annotation.as_ref()?.node);
    }
    if parent_syntax.of::<beskid_analysis::syntax::ReturnStatement>().is_some() {
        let owner = nearest_ancestor(index, parent, |kind| {
            matches!(
                kind,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                    | beskid_analysis::syntax_query::NodeKind::LambdaExpression
            )
        })?;
        let return_type = declared_callable_return_type(index.node_at(program, owner)?)?;
        return function_type_signature(db, AstNodeKey { node: owner, ..lambda }, &return_type.node);
    }
    parent_syntax.of::<beskid_analysis::syntax::CallExpression>()?;
    let arguments = match call_arguments(db, parent_key) {
        Ok(arguments) => arguments?,
        Err(error) => return Some(Err(error)),
    };
    let position =
        arguments.iter().position(|argument| normalized_expression_node(index, argument.node) == lambda.node)?;
    let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, parent_key) else {
        return None;
    };
    let syntax = db.syntax_unit(declaration.unit)?;
    match with_node(db, syntax, declaration, |_program, _index, node| {
        let function = node.of::<beskid_analysis::syntax::FunctionDefinition>()?;
        (function.parameters.len() == arguments.len()).then_some(())?;
        function_type_signature(db, declaration, &function.parameters.get(position)?.node.ty.node)
    }) {
        Ok(expected) => expected,
        Err(error) => Some(Err(error)),
    }
}

fn function_type_signature(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<Result<ItemSignature, SemanticError>> {
    let beskid_analysis::syntax::Type::Function { parameters, return_type } = syntax_type else {
        return None;
    };
    Some((|| {
        let parameters = parameters
            .iter()
            .map(|parameter| abi_type_from_syntax(db, key, &parameter.node))
            .collect::<Result<Vec<_>, _>>()?;
        let result = abi_type_from_syntax(db, key, &return_type.node)?;
        Ok(ItemSignature { parameters: parameters.into(), result })
    })())
}

/// The contextual ABI type of one untyped lambda parameter, by its position in the lambda.
pub(in crate::semantic_contract) fn untyped_lambda_parameter_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    parameter: beskid_analysis::syntax::AstNodeId,
) -> Result<SemanticTypeId, SemanticError> {
    let unavailable = || SemanticError::unavailable("abi_type");
    let lambda = parent_node(index, parameter).ok_or_else(unavailable)?;
    let lambda_syntax = index
        .node_at(program, lambda)
        .and_then(|node| node.of::<beskid_analysis::syntax::LambdaExpression>())
        .ok_or_else(unavailable)?;
    let position = lambda_syntax
        .parameters
        .iter()
        .position(|candidate| {
            index.direct_child_id(program, lambda, beskid_analysis::syntax_query::DynNodeRef::from(candidate))
                == Some(parameter)
        })
        .ok_or_else(unavailable)?;
    let expected = lambda_expected_signature(db, program, index, AstNodeKey { node: lambda, ..key })
        .ok_or_else(unavailable)??
        .parameters;
    (expected.len() == lambda_syntax.parameters.len()).then_some(()).ok_or_else(unavailable)?;
    expected.get(position).copied().ok_or_else(unavailable)
}

/// Whether a lambda's body is a block, whose value (if any) comes from `return` statements.
pub(in crate::semantic_contract) fn lambda_has_statement_body(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    lambda: beskid_analysis::syntax::AstNodeId,
) -> bool {
    index
        .node_at(program, lambda)
        .and_then(|node| node.of::<beskid_analysis::syntax::LambdaExpression>())
        .and_then(|syntax| {
            index.direct_child_id(
                program,
                lambda,
                beskid_analysis::syntax_query::DynNodeRef::from(syntax.body.as_ref()),
            )
        })
        .is_some_and(|body| {
            index.kind(normalized_expression_node(index, body))
                == Some(beskid_analysis::syntax_query::NodeKind::BlockExpression)
        })
}
