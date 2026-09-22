//! Focused call-semantics implementation.

use super::super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn call_lowering_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CallLowering> {
    with_node(db, syntax, key, |program, index, node| call_lowering_for_node(db, program, index, key, node))?
        .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn primitive_numeric_conversion_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<PrimitiveNumericConversion> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let to = primitive_numeric_conversion_target(call)?;
        (call.args.len() == 1).then_some(())?;
        let argument = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&call.args[0]))
            .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })?;
        let from = match abi_type(db, argument) {
            Ok(Some(from)) => from,
            Ok(None) if constant_integer(db, argument).ok().flatten().is_some() => SemanticTypeId::WORD,
            Err(error) if error.is_unavailable() && constant_integer(db, argument).ok().flatten().is_some() => {
                SemanticTypeId::WORD
            }
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

pub(in crate::semantic_contract) fn primitive_numeric_conversion_target(
    call: &beskid_analysis::syntax::CallExpression,
) -> Option<SemanticTypeId> {
    let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
        return None;
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return None;
    };
    Some(match segment.node.name.node.name.as_str() {
        "i32" => SemanticTypeId::I32,
        "i64" => SemanticTypeId::I64,
        "u32" => SemanticTypeId::U32,
        "u8" | "byte" => SemanticTypeId::U8,
        "word" => SemanticTypeId::WORD,
        "f64" => SemanticTypeId::F64,
        _ => return None,
    })
}

#[salsa::tracked(persist)]
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
            && unqualified_enclosing_method_call(program, index, key, &path.node.path.node).is_some()
        {
            let Some(callee) = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            arguments.push(AstNodeKey { node: normalized_expression_node(index, callee), ..key });
        } else if let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node
            && (nominal_local_member_receiver(db, program, index, key, &path.node.path.node).is_some()
                || contract_member_receiver(db, program, index, key, &path.node.path.node).is_some())
        {
            let Some(callee) = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
            ) else {
                return Some(Err(SemanticError::unavailable("call_arguments")));
            };
            // A field-chain receiver is denoted by its own path segment; only an unqualified
            // local receiver is carried by the callee path expression itself.
            let receiver = if path.node.path.node.segments.len() > 2 {
                nominal_local_member_receiver(db, program, index, key, &path.node.path.node)?.1
            } else {
                let implicit_field_root = path.node.path.node.segments.first().is_some_and(|segment| {
                    nominal_local_receiver_declaration(db, program, index, key, segment.node.name.node.name.as_str())
                        .is_none()
                });
                match implicit_field_root
                    .then(|| nominal_local_member_receiver(db, program, index, key, &path.node.path.node))
                    .flatten()
                {
                    Some((_, receiver)) => receiver,
                    None => AstNodeKey { node: normalized_expression_node(index, callee), ..key },
                }
            };
            arguments.push(receiver);
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

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn range_for_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RangeForFact> {
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

#[salsa::tracked(persist)]
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

#[salsa::tracked(persist)]
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
    let expression = node
        .of::<beskid_analysis::syntax::TryExpression>()
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let operand = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(expression.expr.as_ref()))
        .map(|node| normalized_expression_node(index, node))
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let operand = AstNodeKey { node: operand, ..key };
    if index.kind(operand.node) == Some(beskid_analysis::syntax_query::NodeKind::CallExpression) {
        let Some(CallLowering::Direct(declaration)) = call_lowering(db, operand)? else {
            return Err(SemanticError::unavailable("try_expression"));
        };
        if !matches!(
            node_kind(db, declaration)?,
            Some(IndexedNodeKind::FunctionDefinition | IndexedNodeKind::MethodDefinition)
        ) {
            return Err(SemanticError::unavailable("try_expression"));
        }
    } else {
        // A spelling or pointer ABI does not prove an arbitrary member or computed value: the
        // operand must name one lexical declaration (parameter or `let`) whose declared or
        // initialized identity the generic source identity can prove.
        try_operand_declaration(program, index, key, node)?;
    }
    let operand_identity = generic_source_expression_identity(db, operand)?;
    let callable = super::super::locals::enclosing_executable_callable(index, key.node)
        .and_then(|callable| index.node_at(program, callable))
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let return_type = super::super::abi::declared_callable_return_type(callable)
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let result_definition = canonical_result_definition_for_type(db, key, &return_type.node)
        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
    let return_identity = generic_source_type_identity(db, key, &return_type.node)?;
    let (
        GenericSourceTypeIdentity::Nominal { qualified_name: operand_name, arguments: operand_arguments },
        GenericSourceTypeIdentity::Nominal { qualified_name: return_name, arguments: return_arguments },
    ) = (&operand_identity, &return_identity)
    else {
        return Err(SemanticError::unavailable("try_expression"));
    };
    let ([payload, error], [_, return_error]) = (operand_arguments.as_ref(), return_arguments.as_ref()) else {
        return Err(SemanticError::unavailable("try_expression"));
    };
    if operand_name != return_name || error != return_error {
        return Err(SemanticError::unavailable("try_expression"));
    }
    let (operand_definition, operand_layout) =
        super::super::layouts::enum_layout_for_source_identity(db, operand, &operand_identity)?;
    let (return_definition, return_layout) =
        super::super::layouts::enum_layout_for_source_identity(db, key, &return_identity)?;
    if operand_definition != result_definition || return_definition != result_definition {
        return Err(SemanticError::unavailable("try_expression"));
    }

    Ok(TryExpressionFact {
        expression: key,
        operand,
        payload_identity: payload.clone(),
        payload_type: payload.abi_type(),
        error_type: error.abi_type(),
        enclosing_return: return_identity.abi_type(),
        operand_layout,
        return_layout,
        error_managed: error.managed_reference_kind() == ManagedReferenceKind::GcManaged,
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
    let segment = path.node.segments.last()?;
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

/// Resolve the explicitly typed parameter form of syntax `Result` propagation.
///
/// The propagation fact handles proven ordinary calls separately. Local values,
/// member fields, and inferred types do not gain a layout fallback through this guard.
pub(in crate::semantic_contract) fn try_operand_declaration(
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
        .filter(|parent| {
            matches!(
                index.kind(*parent),
                Some(
                    beskid_analysis::syntax_query::NodeKind::Parameter
                        | beskid_analysis::syntax_query::NodeKind::LetStatement
                )
            )
        })
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
    let segment = path.node.segments.last()?;
    if segment.node.name.node.name != "Result" {
        return None;
    }
    let [payload, error] = segment.node.type_args.as_slice() else {
        return None;
    };
    Some((payload, error))
}
