//! Focused call-semantics implementation.

use super::super::*;

#[salsa::tracked]
pub(in crate::semantic_contract) fn call_lowering_tracked(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> SemanticQueryResult<CallLowering> {
    with_node(db, syntax, key, |program, index, node| call_lowering_for_node(db, program, index, key, node))?
        .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn primitive_numeric_conversion_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<PrimitiveNumericConversion> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let [segment] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        let to = match segment.node.name.node.name.as_str() {
            "i32" => SemanticTypeId::I32,
            "i64" => SemanticTypeId::I64,
            "u8" => SemanticTypeId::U8,
            "word" => SemanticTypeId::WORD,
            _ => return None,
        };
        (call.args.len() == 1).then_some(())?;
        let argument = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&call.args[0]))
            .map(|node| AstNodeKey { node, ..key })?;
        let from = match abi_type(db, argument) {
            Ok(Some(from)) => from,
            Ok(None) => return Some(Err(SemanticError::unavailable("primitive_numeric_conversion"))),
            Err(error) => return Some(Err(error)),
        };
        Some(
            primitive_integer(from)
                .then_some(PrimitiveNumericConversion { from, to })
                .ok_or_else(|| SemanticError::unavailable("primitive_numeric_conversion")),
        )
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn call_arguments_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let mut arguments = Vec::with_capacity(call.args.len() + 1);
        if let beskid_analysis::syntax::Expression::Member(member) = &call.callee.node {
            let Some(callee) = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            let callee = normalized_expression_node(index, callee);
            let Some(receiver) = index.direct_child_id(
                program,
                callee,
                beskid_analysis::syntax_query::DynNodeRef::from(member.node.target.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            arguments.push(AstNodeKey { node: normalized_expression_node(index, receiver), ..key });
        } else if let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node
            && nominal_local_member_receiver(db, program, index, key, &path.node.path.node).is_some()
        {
            let Some(callee) = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            arguments.push(AstNodeKey { node: normalized_expression_node(index, callee), ..key });
        }
        let explicit = match call
            .args
            .iter()
            .map(|argument| {
                index
                    .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                    .map(|node| AstNodeKey { node, ..key })
                    .ok_or_else(|| SemanticError::unavailable("call_arguments"))
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(explicit) => explicit,
            Err(error) => return Some(Err(error)),
        };
        arguments.extend(explicit);
        Some(Ok(arguments.into()))
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn range_for_fact_tracked(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> SemanticQueryResult<RangeForFact> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let [segment] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        if segment.node.name.node.name != "range" || !segment.node.type_args.is_empty() {
            return None;
        }
        let [start, end] = call.args.as_slice() else {
            return Some(Err(SemanticError::unavailable("range_for_fact")));
        };
        let start = index.direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(start))?;
        let end = index.direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(end))?;
        Some(Ok(RangeForFact {
            start: AstNodeKey { node: normalized_expression_node(index, start), ..key },
            end: AstNodeKey { node: normalized_expression_node(index, end), ..key },
        }))
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn for_iterator_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ForIteratorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let statement = node.of::<beskid_analysis::syntax::ForStatement>()?;
        let declaration = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(&statement.iterator),
        )?;
        match element_type_for_for_iterable(program, index, key.node, &statement.iterable.node) {
            Ok(element_type) => {
                Some(Ok(ForIteratorFact { declaration: AstNodeKey { node: declaration, ..key }, element_type }))
            }
            Err(error) => Some(Err(error)),
        }
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn try_expression_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<TryExpressionFact> {
    with_node(db, syntax, key, |program, index, node| {
        Some(try_expression_fact_for_node(db, program, index, key, node))
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn try_expression_fact_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Result<TryExpressionFact, SemanticError> {
    let (operand, declaration) = try_operand_parameter_declaration(program, index, key, node)?;
    let parameter = parent_node(index, declaration)
        .filter(|parent| index.kind(*parent) == Some(beskid_analysis::syntax_query::NodeKind::Parameter))
        .and_then(|parent| {
            index.node_at(program, parent).and_then(|node| node.of::<beskid_analysis::syntax::Parameter>())
        })
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let parameter_type = &parameter.ty;
    let result_definition = canonical_result_definition_for_type(db, key, &parameter_type.node)
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let (payload, error) =
        result_type_parts(&parameter_type.node).ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let function =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
            .and_then(|function| {
                index
                    .node_at(program, function)
                    .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
            })
            .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let return_type = function.return_type.as_ref().ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    if canonical_result_definition_for_type(db, key, &return_type.node) != Some(result_definition) {
        return Err(SemanticError::unavailable("try_expression"));
    }
    if !same_type_syntax(&parameter_type.node, &return_type.node) {
        return Err(SemanticError::unavailable("try_expression"));
    }

    Ok(TryExpressionFact {
        expression: key,
        operand: AstNodeKey { node: operand, ..key },
        payload_type: semantic_type_from_syntax(&payload.node)?,
        error_type: semantic_type_from_syntax(&error.node)?,
        enclosing_return: semantic_type_from_syntax(&return_type.node)?,
    })
}

/// Resolve the one Result definition that generated `value?` lowering may trust.
///
/// A `Result<T, E>` spelling alone is not an ABI contract: a user declaration with the same
/// name can reorder, add, or change variants. The propagation emitter assumes exactly the
/// canonical `Ok(T value), Error(E error)` two-variant object representation, so establish the
/// declaration identity and its generic field shapes before it receives any layout facts.
pub(in crate::semantic_contract) fn canonical_result_definition_for_type(
    db: &dyn Db,
    use_key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<AstNodeKey> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    if segment.node.name.node.name != "Result" || segment.node.type_args.len() != 2 {
        return None;
    }
    let declaration = resolve_type_declaration(db, use_key, &path.node)?;
    let syntax = db.syntax_unit(declaration.unit)?;
    if syntax.generation(db) != declaration.generation {
        return None;
    }
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<beskid_analysis::syntax::EnumDefinition>()?;
    let [payload_parameter, error_parameter] = definition.generics.as_slice() else {
        return None;
    };
    let [ok, error] = definition.variants.as_slice() else {
        return None;
    };
    (definition.name.node.name == "Result"
        && canonical_result_variant(&ok.node, "Ok", "value", payload_parameter)
        && canonical_result_variant(&error.node, "Error", "error", error_parameter))
    .then_some(declaration)
}

pub(in crate::semantic_contract) fn canonical_result_variant(
    variant: &beskid_analysis::syntax::EnumVariant,
    name: &str,
    field_name: &str,
    generic_parameter: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Identifier>,
) -> bool {
    let [field] = variant.fields.as_slice() else {
        return false;
    };
    variant.name.node.name == name
        && field.node.kind == beskid_analysis::syntax::FieldKind::Value
        && field.node.name.node.name == field_name
        && type_syntax_is_generic_parameter_reference(&field.node.ty.node, generic_parameter.node.name.as_str())
}

/// Resolve the only operand shape currently eligible for syntax `Result` propagation.
///
/// Reusing this guard for both the propagation fact and the concrete enum-layout query keeps
/// layout authority tied to the same direct, explicitly typed function parameter; local values,
/// calls, members, and inferred types do not gain a layout fallback.
pub(in crate::semantic_contract) fn try_operand_parameter_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Result<(beskid_analysis::syntax::AstNodeId, beskid_analysis::syntax::AstNodeId), SemanticError> {
    let try_expression = node
        .of::<beskid_analysis::syntax::TryExpression>()
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let operand = index
        .direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(try_expression.expr.as_ref()),
        )
        .map(|node| normalized_expression_node(index, node))
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let operand_node = index.node_at(program, operand).ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let path = operand_node
        .of::<beskid_analysis::syntax::PathExpression>()
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let [segment] = path.path.node.segments.as_slice() else {
        return Err(SemanticError::unavailable("try_expression"));
    };
    if !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("try_expression"));
    }
    let declaration = resolve_lexical_declaration(program, index, operand, segment.node.name.node.name.as_str())
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    parent_node(index, declaration)
        .filter(|parent| index.kind(*parent) == Some(beskid_analysis::syntax_query::NodeKind::Parameter))
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    Ok((operand, declaration))
}

pub(in crate::semantic_contract) fn result_type_parts(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<(
    &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>,
    &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>,
)> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    if segment.node.name.node.name != "Result" {
        return None;
    }
    let [payload, error] = segment.node.type_args.as_slice() else {
        return None;
    };
    Some((payload, error))
}

pub(in crate::semantic_contract) fn same_type_syntax(left: &beskid_analysis::syntax::Type, right: &beskid_analysis::syntax::Type) -> bool {
    use beskid_analysis::syntax::Type;

    match (left, right) {
        (Type::Primitive(left), Type::Primitive(right)) => left.node == right.node,
        (Type::Complex(left), Type::Complex(right)) => {
            left.node.segments.len() == right.node.segments.len()
                && left.node.segments.iter().zip(&right.node.segments).all(|(left, right)| {
                    left.node.name.node.name == right.node.name.node.name
                        && left.node.type_args.len() == right.node.type_args.len()
                        && left
                            .node
                            .type_args
                            .iter()
                            .zip(&right.node.type_args)
                            .all(|(left, right)| same_type_syntax(&left.node, &right.node))
                })
        }
        (Type::Array(left), Type::Array(right)) => same_type_syntax(&left.node, &right.node),
        (
            Type::Function { return_type: left_return, parameters: left_parameters },
            Type::Function { return_type: right_return, parameters: right_parameters },
        ) => {
            same_type_syntax(&left_return.node, &right_return.node)
                && left_parameters.len() == right_parameters.len()
                && left_parameters
                    .iter()
                    .zip(right_parameters)
                    .all(|(left, right)| same_type_syntax(&left.node, &right.node))
        }
        _ => false,
    }
}

