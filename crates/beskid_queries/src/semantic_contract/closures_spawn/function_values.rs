//! Function values.
//!
//! A function value is a reference to a GC-managed closure record `{ header, code pointer,
//! environment pointer }`. Three facts describe them:
//!
//! - [`function_value_call_tracked`]: a call whose callee is not a statically known item but an
//!   expression of function type (a function-typed local or parameter, a function-typed field
//!   projection, a call that returns a function value, or any other expression whose source type
//!   is a function type). The record is read at the call site and invoked indirectly.
//! - [`lambda_value_required_tracked`]: whether a lambda expression must materialize its record.
//! - [`function_item_value_tracked`]: a named, non-generic function item read as a value; its
//!   record carries the item's own entry and a null environment.
//!
//! Lambda typing is one checking-direction fact: a lambda's own source identity is the function
//! type its position declares, through [`expected_type`], for every untyped parameter and for a
//! block body's `return`. Typed parameters and an expression body keep synthesis. There is no
//! second parent walk.

use super::super::layouts::explicit_local_declaration_type;
use super::super::*;
use super::*;

/// Return the normalized callee key when the callee of `call` is a function value.
///
/// A lambda callee is an inline call and never a function value. A single-segment path callee is
/// a function value when it names a lexical declaration of function type, or an unannotated local
/// whose initializer is a lambda; such a local shadows every item, import and builtin of its
/// name. Any other callee whose source identity is a function type (a function-typed field
/// projection, a call returning a function value, a parenthesized expression) is a function
/// value. A path that names no lexical declaration is an item or module path, never a value.
pub(in crate::semantic_contract) fn function_value_callee(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    call: &beskid_analysis::syntax::CallExpression,
) -> Option<AstNodeKey> {
    if expression_is_lambda(&call.callee.node) {
        return None;
    }
    let callee = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()))
        .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })?;
    if let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node {
        let segments = path.node.path.node.segments.as_slice();
        let first = segments.first()?;
        if !first.node.type_args.is_empty() {
            return None;
        }
        if resolve_lexical_declaration(program, index, key.node, first.node.name.node.name.as_str()).is_none() {
            return None;
        }
        if segments.len() == 1 && function_value_declaration(program, index, key.node, &path.node.path.node).is_some()
        {
            return Some(callee);
        }
    }
    matches!(generic_source_expression_identity(db, callee), Ok(GenericSourceTypeIdentity::Function { .. }))
        .then_some(callee)
}

/// Return the function-typed lexical declaration named by a single-segment call callee: a
/// parameter, typed lambda parameter or annotated local declared with a function type, or an
/// unannotated local initialized by a lambda.
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
    let parent = parent_node(index, declaration)?;
    let parent_syntax = index.node_at(program, parent)?;
    let declared = explicit_local_declaration_type(program, index, declaration).or_else(|| {
        parent_syntax
            .of::<beskid_analysis::syntax::LambdaParameter>()
            .and_then(|parameter| parameter.ty.as_ref().map(|ty| &ty.node))
    });
    if let Some(declared) = declared {
        return matches!(declared, beskid_analysis::syntax::Type::Function { .. }).then_some(declaration);
    }
    let binding = parent_syntax.of::<beskid_analysis::syntax::LetStatement>()?;
    expression_is_lambda(&binding.value.node).then_some(declaration)
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn function_value_call_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<FunctionValueCall> {
    function_value_call_in_environment(db, syntax, key, None)
}

/// [`function_value_call`] inside one concrete generic specialization of its enclosing item.
///
/// A callee whose declared function type names the enclosing declaration's generic parameters
/// (for example `(T) => T f` in `T Apply<T>(...)`) has no context-free ABI; the instance's
/// substitutions bind them.
pub fn function_value_call_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<FunctionValueCall> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    function_value_call_in_environment(db, syntax, key, Some(enclosing))
}

fn function_value_call_in_environment(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<FunctionValueCall> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let callee = function_value_callee(db, program, index, key, call)?;
        Some(specialized_source_expression_identity(db, callee, enclosing).and_then(|identity| {
            let GenericSourceTypeIdentity::Function { parameters, result } = identity else {
                return Err(SemanticError::unavailable("function_value_call"));
            };
            Ok(FunctionValueCall {
                call: key,
                callee,
                signature: ItemSignature {
                    parameters: parameters.iter().map(GenericSourceTypeIdentity::abi_type).collect(),
                    result: result.abi_type(),
                },
                result_managed_reference: result.managed_reference_kind(),
                result_identity: *result,
            })
        }))
    })?
    .transpose()
}

/// Decide whether a lambda expression must materialize its closure record.
///
/// Immediate calls, spawn operands, and event-handler locals have their own lowering. A local
/// bound to a lambda needs a record only when it is mutable, has a block body, or when some use
/// of it is not the callee of a call; calls through an immutable expression-bodied lambda local
/// stay inline. Every other lambda position (an argument, a field value, a return value, an
/// assigned value) is a function value.
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

/// A named function item read as a value: a path expression that is not a call callee and
/// resolves to a non-generic `FunctionDefinition`. Generic items have no context-free entry and
/// fail closed; methods are not values.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn function_item_value_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<FunctionItemValue> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let parent = value_use_parent(index, key.node)?;
        if is_call_callee(program, index, parent, key.node) {
            return None;
        }
        let declaration = resolve_item_declaration(db, program, index, key, &path.path.node)?;
        let item_syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
        let function = item_syntax
            .syntax_index(db)
            .node_at(item_syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::FunctionDefinition>()?;
        if !function.generics.is_empty() {
            return Some(Err(SemanticError::unavailable("function_item_value")));
        }
        Some(item_abi_signature(db, declaration).and_then(|signature| {
            signature
                .map(|signature| FunctionItemValue { path: key, declaration, signature })
                .ok_or_else(|| SemanticError::unavailable("function_item_value"))
        }))
    })?
    .transpose()
}

/// The function type of a named function item used as a value, from its declared signature.
pub(in crate::semantic_contract) fn function_item_value_identity(
    db: &dyn Db,
    value: &FunctionItemValue,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let unavailable = || SemanticError::unavailable("function_item_value");
    let syntax = db.syntax_unit(value.declaration.unit).filter(|syntax| syntax.accepts_key(db, value.declaration));
    let syntax = syntax.ok_or_else(unavailable)?;
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), value.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .ok_or_else(unavailable)?;
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| generic_source_type_identity(db, value.declaration, &parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result = match function.return_type.as_ref() {
        Some(return_type) => generic_source_type_identity(db, value.declaration, &return_type.node)?,
        None => GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT),
    };
    Ok(GenericSourceTypeIdentity::Function { parameters: parameters.into(), result: Box::new(result) })
}

/// The source identity of a lambda expression: its function type.
///
/// A typed parameter contributes its declared type. An untyped parameter and a block body (whose
/// value comes from `return`) take the function type the lambda's position declares through
/// [`expected_type`]; a block body without one is `unit`. An expression body synthesizes its own
/// identity (its bare literals are checked against the expected result through their own
/// [`expected_type`], so no result is guessed here).
pub(in crate::semantic_contract) fn lambda_source_identity(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let lambda = index
        .node_at(program, key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::LambdaExpression>())
        .ok_or_else(|| source_expression_unavailable(db, key, "lambda node is absent"))?;
    let body = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(lambda.body.as_ref()))
        .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
        .ok_or_else(|| source_expression_unavailable(db, key, "lambda body node is absent"))?;
    let statement_body = index.kind(body.node) == Some(beskid_analysis::syntax_query::NodeKind::BlockExpression);
    let untyped = lambda.parameters.iter().any(|parameter| parameter.node.ty.is_none());
    let expected = if statement_body || untyped { lambda_expected_function(db, key, lambda.parameters.len())? } else { None };
    let parameters = lambda
        .parameters
        .iter()
        .enumerate()
        .map(|(position, parameter)| match (&parameter.node.ty, &expected) {
            (Some(ty), _) => generic_source_type_identity(db, key, &ty.node),
            (None, Some((parameters, _))) => Ok(parameters[position].clone()),
            (None, None) => Err(source_expression_unavailable(
                db,
                key,
                "untyped lambda parameter has no expected function type",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = if statement_body {
        expected.map_or(GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT), |(_, result)| result)
    } else {
        // A pointer-shaped body keeps its source identity; a scalar body takes its value ABI,
        // which checks a bare literal against the expected result through the body's own
        // `expected_type`.
        match generic_source_expression_identity(db, body) {
            Ok(identity @ (GenericSourceTypeIdentity::Nominal { .. }
            | GenericSourceTypeIdentity::Array(_)
            | GenericSourceTypeIdentity::Function { .. })) => identity,
            Ok(GenericSourceTypeIdentity::Abi(_)) | Err(_) => GenericSourceTypeIdentity::Abi(
                value_abi_type(db, body)?
                    .ok_or_else(|| source_expression_unavailable(db, body, "lambda body has no value type"))?,
            ),
        }
    };
    Ok(GenericSourceTypeIdentity::Function { parameters: parameters.into(), result: Box::new(result) })
}

/// The parameter and result identities of the function type a lambda's position declares, when
/// its arity matches the lambda. A position without a function-typed expectation yields `None`.
fn lambda_expected_function(
    db: &dyn Db,
    lambda: AstNodeKey,
    arity: usize,
) -> Result<Option<(Vec<GenericSourceTypeIdentity>, GenericSourceTypeIdentity)>, SemanticError> {
    let expected = match expected_type(db, lambda) {
        Ok(Some(expected)) => expected,
        Ok(None) => return Ok(None),
        Err(error) if error.is_unavailable() => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(expected
        .function_identity()
        .filter(|(parameters, _)| parameters.len() == arity)
        .map(|(parameters, result)| (parameters.to_vec(), result.clone())))
}

/// The contextual source identity of one untyped lambda parameter, by its position in the lambda.
pub(in crate::semantic_contract) fn untyped_lambda_parameter_identity(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    parameter: beskid_analysis::syntax::AstNodeId,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
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
    let (parameters, _) = lambda_expected_function(db, AstNodeKey { node: lambda, ..key }, lambda_syntax.parameters.len())?
        .ok_or_else(unavailable)?;
    parameters.into_iter().nth(position).ok_or_else(unavailable)
}

/// The contextual ABI type of one untyped lambda parameter, by its position in the lambda.
pub(in crate::semantic_contract) fn untyped_lambda_parameter_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    parameter: beskid_analysis::syntax::AstNodeId,
) -> Result<SemanticTypeId, SemanticError> {
    untyped_lambda_parameter_identity(db, program, index, key, parameter).map(|identity| identity.abi_type())
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

/// Whether `node` is the (normalized) callee of the call expression `parent`.
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
