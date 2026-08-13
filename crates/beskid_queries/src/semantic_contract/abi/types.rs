//! Focused ABI semantic implementation.

use super::super::*;

/// Prove an ABI representation for one unsuffixed integer literal at an exact declared boundary:
/// an explicitly typed local, mutable-local assignment, enum-variant payload, or nominal
/// struct-literal field.
///
/// This is contextual typing for literals, not a conversion: the source literal has no ABI suffix
/// and is emitted directly at the destination's exact ABI width. Inferred declarations, explicit
/// literal suffixes, compound expressions, immutable destinations, and non-integer destination
/// representations fail closed rather than receiving an implicit numeric widening.
#[salsa::tracked]
pub(in crate::semantic_contract) fn contextual_integer_literal_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, _node| {
        Some((|| {
            let contextual_constant = contextual_constant_integer(db, key)?.is_some();
            if !unsuffixed_integer_literal(db, key)? && !contextual_constant {
                return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
            }
            let mut current = key.node;
            while let Some(parent) = index.metadata_for(key.generation, current).and_then(|meta| meta.parent) {
                let parent_key = AstNodeKey { node: parent, ..key };
                let parent_syntax = index
                    .node_at(program, parent)
                    .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;

                if let Some(binding) = parent_syntax.of::<beskid_analysis::syntax::LetStatement>() {
                    let initializer = index
                        .direct_child_id(
                            program,
                            parent,
                            beskid_analysis::syntax_query::DynNodeRef::from(&binding.value),
                        )
                        .map(|node| AstNodeKey { node, ..key })
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    if integer_literal_text(db, initializer)?.is_none()
                        && contextual_constant_integer(db, initializer)?.is_none()
                    {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    }
                    let annotation = binding
                        .type_annotation
                        .as_ref()
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let expected = abi_type_from_syntax(db, parent_key, &annotation.node)?;
                    return (contextual_constant_integer(db, initializer)?.is_some()
                        || integer_literal_fits_abi(db, initializer, expected)?)
                    .then_some(expected)
                    .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"));
                }

                if let Some(assignment) = parent_syntax.of::<beskid_analysis::syntax::AssignExpression>() {
                    let value = index
                        .direct_child_id(
                            program,
                            parent,
                            beskid_analysis::syntax_query::DynNodeRef::from(assignment.value.as_ref()),
                        )
                        .map(|node| AstNodeKey { node, ..key })
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    if integer_literal_text(db, value)?.is_none() && contextual_constant_integer(db, value)?.is_none() {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    }
                    let write = mutable_local_assignment(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let expected = abi_type(db, write.declaration)?
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    return (contextual_constant_integer(db, value)?.is_some()
                        || integer_literal_fits_abi(db, value, expected)?)
                    .then_some(expected)
                    .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"));
                }

                if let Some(constructor) = parent_syntax.of::<beskid_analysis::syntax::EnumConstructorExpression>() {
                    let [argument] = constructor.args.as_slice() else {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    };
                    let payload = index
                        .direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                        .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    if payload != key {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    }
                    let layout = enum_layout(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let variant_name = constructor.path.node.variant.node.name.as_str();
                    let variant = layout
                        .variants
                        .iter()
                        .find(|variant| variant.name.as_ref() == variant_name)
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let [(_, AggregateFieldShape::Scalar(expected))] = variant.fields.as_ref() else {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    };
                    return (primitive_integer(*expected)
                        && (contextual_constant_integer(db, payload)?.is_some()
                            || integer_literal_fits_abi(db, payload, *expected)?))
                    .then_some(*expected)
                    .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"));
                }

                if parent_syntax.of::<beskid_analysis::syntax::ReturnStatement>().is_some() {
                    if integer_literal_text(db, key)?.is_none() && contextual_constant_integer(db, key)?.is_none() {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    }
                    let mut item = parent;
                    while !matches!(
                        index.kind(item),
                        Some(
                            beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                                | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                        )
                    ) {
                        item = parent_node(index, item)
                            .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    }
                    let item_key = AstNodeKey { node: item, ..key };
                    let item_syntax = index
                        .node_at(program, item)
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let annotation = item_syntax
                        .of::<beskid_analysis::syntax::FunctionDefinition>()
                        .and_then(|function| function.return_type.as_ref().map(|ty| &ty.node))
                        .or_else(|| {
                            item_syntax
                                .of::<beskid_analysis::syntax::MethodDefinition>()
                                .and_then(|method| method.return_type.as_ref().map(|ty| &ty.node))
                        })
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let expected = abi_type_from_syntax(db, item_key, annotation)?;
                    return (primitive_integer(expected)
                        && (contextual_constant_integer(db, key)?.is_some()
                            || integer_literal_fits_abi(db, key, expected)?))
                    .then_some(expected)
                    .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"));
                }

                if let Some(field) = parent_syntax.of::<beskid_analysis::syntax::StructLiteralField>() {
                    let value = index
                        .direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(&field.value))
                        .map(|node| AstNodeKey { node, ..key })
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    if integer_literal_text(db, value)?.is_none() && contextual_constant_integer(db, value)?.is_none() {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    }
                    let literal_node = parent_node(index, parent)
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    if index.kind(literal_node)
                        != Some(beskid_analysis::syntax_query::NodeKind::StructLiteralExpression)
                    {
                        return Err(SemanticError::unavailable("contextual_integer_literal_abi_type"));
                    }
                    let literal_key = AstNodeKey { node: literal_node, ..key };
                    let declaration = aggregate_literal_declaration(db, literal_key)?
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let declaration_syntax = db
                        .syntax_unit(declaration.unit)
                        .filter(|unit| unit.generation(db) == declaration.generation)
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let definition = declaration_syntax
                        .syntax_index(db)
                        .node_at(declaration_syntax.expanded_program(db), declaration.node)
                        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let declared = definition
                        .fields
                        .iter()
                        .find(|candidate| {
                            candidate.node.kind == beskid_analysis::syntax::FieldKind::Value
                                && candidate.node.name.node.name == field.name.node.name
                        })
                        .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"))?;
                    let expected = abi_type_from_syntax(db, declaration, &declared.node.ty.node)?;
                    return (primitive_integer(expected)
                        && (contextual_constant_integer(db, value)?.is_some()
                            || integer_literal_fits_abi(db, value, expected)?))
                    .then_some(expected)
                    .ok_or_else(|| SemanticError::unavailable("contextual_integer_literal_abi_type"));
                }

                current = parent;
            }
            Err(SemanticError::unavailable("contextual_integer_literal_abi_type"))
        })())
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(expression) = node.of::<beskid_analysis::syntax::Expression>() {
            return Some(abi_type_for_expression(db, program, index, key, expression));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
            return Some(Ok(semantic_type_for_literal(literal)));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
            return Some(Ok(semantic_type_for_literal(&literal.literal.node)));
        }
        if let Some(statement) = node.of::<beskid_analysis::syntax::LetStatement>() {
            return Some(
                statement
                    .type_annotation
                    .as_ref()
                    .ok_or_else(|| SemanticError::unavailable("abi_type"))
                    .and_then(|ty| abi_type_from_syntax(db, key, &ty.node)),
            );
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
            return Some(abi_type_for_local_path(db, program, index, key, &path.path.node));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        if node.of::<beskid_analysis::syntax::AssignExpression>().is_some() {
            // An index assignment is expression-valued only after the same declared-array fact
            // that authorizes its element store proves the destination representation. Other
            // assignment shapes intentionally retain no syntax ABI fact here.
            return match array_index_element_abi_type(db, key) {
                Ok(Some(element)) => Some(Ok(element)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            };
        }
        if node.of::<beskid_analysis::syntax::Identifier>().is_some() {
            return abi_local_declaration_type(db, program, index, key, key.node);
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
            match primitive_numeric_conversion(db, key) {
                Ok(Some(conversion)) => return Some(Ok(conversion.to)),
                Ok(None) => (),
                Err(error) => return Some(Err(error)),
            }
            let lowering = match call_lowering(db, key) {
                Ok(Some(lowering)) => lowering,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            if !matches!(lowering, CallLowering::Direct(_) | CallLowering::Runtime(_)) {
                return Some(Err(SemanticError::unavailable("abi_type")));
            }
            let signature = match call_abi_signature(db, key) {
                Ok(Some(signature)) => signature,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            return Some(Ok(signature.result));
        }
        if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
            return Some(abi_type_from_syntax(db, key, syntax_type));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
            return Some(Ok(semantic_type_for_literal(literal)));
        }
        None
    })?
    .transpose()
}

/// One generation-bound ABI representation fact for source values and declared storage
/// boundaries. This composes only existing syntax facts; it never reconstructs HIR types.
#[salsa::tracked]
pub(in crate::semantic_contract) fn value_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |_program, _index, _node| {
        Some((|| {
            let contextual = optional_abi_fact(contextual_integer_literal_abi_type(db, key))?;
            let call_argument = optional_abi_fact(call_argument_abi_type(db, key))?;
            let binary_operand = optional_abi_fact(binary_operand_abi_type(db, key))?;
            let call_result = optional_abi_fact(call_abi_signature(db, key))?.map(|signature| signature.result);
            let abi = optional_abi_fact(abi_type(db, key))?;
            let semantic = optional_abi_fact(node_type(db, key))?;
            contextual
                .or(call_argument)
                .or(binary_operand)
                .or(call_result)
                .or(abi)
                .or(semantic)
                .ok_or_else(|| SemanticError::unavailable("value_abi_type"))
        })())
    })?
    .transpose()
}

fn optional_abi_fact<T>(result: SemanticQueryResult<T>) -> Result<Option<T>, SemanticError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) if error.is_unavailable() => Ok(None),
        Err(error) => Err(error),
    }
}

pub(in crate::semantic_contract) fn abi_type_for_expression(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::Expression;

    match expression {
        Expression::Literal(literal) => Ok(semantic_type_for_literal(&literal.node.literal.node)),
        Expression::Path(path) => abi_type_for_local_path(db, program, index, key, &path.node.path.node),
        Expression::Grouped(grouped) => abi_type_for_expression(db, program, index, key, &grouped.node.expr.node),
        Expression::Call(call) => {
            let call = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            call_abi_signature(db, call)?
                .map(|signature| signature.result)
                .ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Binary(binary) => {
            let binary_key = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(binary))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            abi_type(db, binary_key)?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

pub(in crate::semantic_contract) fn abi_type_for_binary_expression(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    binary: &beskid_analysis::syntax::BinaryExpression,
) -> Result<SemanticTypeId, SemanticError> {
    let left = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(binary.left.as_ref()))
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let right = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(binary.right.as_ref()))
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let left_type = value_abi_type(db, left)?.ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let right_type = value_abi_type(db, right)?.ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    if left_type != right_type {
        if integer_literal_text(db, left)?.is_some() && primitive_integer(right_type) {
            integer_literal_fits_abi(db, left, right_type)?
                .then_some(())
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            return binary_result_type(binary.op.node, right_type);
        }
        if integer_literal_text(db, right)?.is_some() && primitive_integer(left_type) {
            integer_literal_fits_abi(db, right, left_type)?
                .then_some(())
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            return binary_result_type(binary.op.node, left_type);
        }
    }
    use beskid_analysis::syntax::BinaryOp;
    match binary.op.node {
        BinaryOp::Add if left_type == SemanticTypeId::STRING || right_type == SemanticTypeId::STRING => {
            Ok(SemanticTypeId::STRING)
        }
        BinaryOp::Eq | BinaryOp::NotEq
            if left_type == SemanticTypeId::STRING && right_type == SemanticTypeId::STRING =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Or | BinaryOp::And if left_type == SemanticTypeId::BOOL && right_type == SemanticTypeId::BOOL => {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte
            if left_type == right_type =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if left_type == right_type && primitive_numeric(left_type) =>
        {
            Ok(left_type)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr
            if left_type == right_type && primitive_integer(left_type) =>
        {
            Ok(left_type)
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

fn binary_result_type(
    operator: beskid_analysis::syntax::BinaryOp,
    operand: SemanticTypeId,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::BinaryOp;

    match operator {
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte => Ok(SemanticTypeId::BOOL),
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod if primitive_numeric(operand) => {
            Ok(operand)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr if primitive_integer(operand) => Ok(operand),
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

pub(in crate::semantic_contract) fn abi_type_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::Type;

    match syntax_type {
        Type::Primitive(_) => semantic_type_from_syntax(syntax_type),
        Type::Complex(path) => nominal_aggregate_abi_type(db, key, &path.node),
        Type::Array(_) => Ok(SemanticTypeId::POINTER),
        Type::Function { .. } => Err(SemanticError::unavailable("abi_type")),
    }
}
