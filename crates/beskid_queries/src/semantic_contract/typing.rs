//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked]
pub(super) fn node_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
            match primitive_numeric_conversion(db, key) {
                Ok(Some(conversion)) => return Some(Ok(conversion.to)),
                Ok(None) => (),
                Err(error) => return Some(Err(error)),
            }
            match call_lowering(db, key) {
                Ok(Some(CallLowering::Direct(_) | CallLowering::Runtime(_))) => (),
                Ok(Some(_) | None) => return Some(Err(SemanticError::unavailable("node_type"))),
                Err(error) => return Some(Err(error)),
            };
            return Some(
                call_abi_signature(db, key)
                    .and_then(|signature| signature.ok_or_else(|| SemanticError::unavailable("node_type")))
                    .map(|signature| signature.result),
            );
        }
        if let Some(binding_type) = pattern_binding_semantic_type(db, program, index, key, node) {
            return Some(binding_type);
        }
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(SemanticTypeId::I32));
        }
        if node.of::<beskid_analysis::syntax::MatchExpression>().is_some() && matches!(enum_match(db, key), Ok(Some(_)))
        {
            return Some(enum_match_result_semantic_type(db, key));
        }
        if let Some(beskid_analysis::syntax::Expression::Call(call)) = node.of::<beskid_analysis::syntax::Expression>()
        {
            let call = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("node_type"));
            return Some(call.and_then(|call| {
                call_abi_signature(db, call)?
                    .map(|signature| signature.result)
                    .ok_or_else(|| SemanticError::unavailable("node_type"))
            }));
        }
        semantic_type_for_node(program, index, key.node, node)
    })?
    .transpose()
}

pub(super) fn enum_match_result_semantic_type(db: &dyn Db, key: AstNodeKey) -> Result<SemanticTypeId, SemanticError> {
    let fact = enum_match(db, key)?.ok_or_else(|| SemanticError::unavailable("node_type"))?;
    let mut result = None;
    for arm in fact.arms.iter() {
        let arm_type = contextual_integer_literal_abi_type(db, arm.body)?
            .or(node_type(db, arm.body)?)
            .ok_or_else(|| SemanticError::unavailable("node_type"))?;
        if result.replace(arm_type).is_some_and(|previous| previous != arm_type) {
            return Err(SemanticError::unavailable("node_type"));
        }
    }
    result.ok_or_else(|| SemanticError::unavailable("node_type"))
}

pub(super) fn pattern_binding_semantic_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
    pattern_binding_abi_type(db, index, key, declaration)
}

/// Derive the ABI representation of an enum-pattern binding from its already-resolved match
/// layout. Both ordinary expression typing and generic-call specialization consume this fact.
pub(crate) fn pattern_binding_abi_type(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let binding = match pattern_binding_fact(db, index, key, declaration)? {
        Ok(binding) => binding,
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(match binding.payload {
        AggregateFieldShape::Scalar(semantic) => semantic,
        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
    }))
}

pub(super) fn pattern_binding_fact(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<EnumMatchBindingFact, SemanticError>> {
    if index.kind(parent_node(index, declaration)?)? != beskid_analysis::syntax_query::NodeKind::Pattern {
        return None;
    }
    let arm = nearest_ancestor(index, declaration, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchArm)?;
    let outer_match =
        nearest_ancestor(index, arm, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchExpression)?;
    if outer_match == key.node {
        return None;
    }
    let outer_match = AstNodeKey { node: outer_match, ..key };
    let fact = match enum_match(db, outer_match) {
        Ok(Some(fact)) => fact,
        Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("pattern_binding"))),
    };
    fact.arms
        .iter()
        .filter_map(|arm| arm.binding)
        .find(|binding| binding.declaration.node == declaration)
        .map(Ok)
        .or_else(|| Some(Err(SemanticError::unavailable("pattern_binding"))))
}

pub(super) fn semantic_type_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
        return Some(Ok(semantic_type_for_literal(literal)));
    }
    if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
        return Some(Ok(semantic_type_for_literal(&literal.literal.node)));
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        return Some(semantic_type_for_local_path(program, index, reference, &path.path.node));
    }
    if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
        return Some(semantic_type_for_binary_operands(
            program,
            index,
            reference,
            &binary.left.node,
            binary.op.node,
            &binary.right.node,
        ));
    }
    if let Some(match_expression) = node.of::<beskid_analysis::syntax::MatchExpression>() {
        let mut result = None;
        for arm in &match_expression.arms {
            let arm_type = match semantic_type_for_expression(program, index, reference, &arm.node.value.node) {
                Ok(arm_type) => arm_type,
                Err(error) => return Some(Err(error)),
            };
            if result.replace(arm_type).is_some_and(|previous| previous != arm_type) {
                return Some(Err(SemanticError::unavailable("node_type")));
            }
        }
        return result.map(Ok).or_else(|| Some(Err(SemanticError::unavailable("node_type"))));
    }
    if let Some(expression) = node.of::<beskid_analysis::syntax::Expression>() {
        return Some(semantic_type_for_expression(program, index, reference, expression));
    }
    if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
        return Some(semantic_type_from_syntax(syntax_type));
    }
    if node.of::<beskid_analysis::syntax::Identifier>().is_some() {
        return local_declaration_type(program, index, reference);
    }
    expression_fact_target(node.node_kind()).then(|| Err(SemanticError::unavailable("node_type")))
}

pub(super) fn semantic_type_for_expression(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    expression: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    match expression {
        beskid_analysis::syntax::Expression::Literal(literal) => {
            Ok(semantic_type_for_literal(&literal.node.literal.node))
        }
        beskid_analysis::syntax::Expression::Path(path) => {
            semantic_type_for_local_path(program, index, reference, &path.node.path.node)
        }
        beskid_analysis::syntax::Expression::Grouped(grouped) => {
            semantic_type_for_expression(program, index, reference, &grouped.node.expr.node)
        }
        beskid_analysis::syntax::Expression::Binary(binary) => semantic_type_for_binary_operands(
            program,
            index,
            reference,
            &binary.node.left.node,
            binary.node.op.node,
            &binary.node.right.node,
        ),
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

pub(super) fn semantic_type_for_binary_operands(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    left: &beskid_analysis::syntax::Expression,
    op: beskid_analysis::syntax::BinaryOp,
    right: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    let left = semantic_type_for_expression(program, index, reference, left)?;
    let right = semantic_type_for_expression(program, index, reference, right)?;
    use beskid_analysis::syntax::BinaryOp;
    match op {
        BinaryOp::Or | BinaryOp::And if left == SemanticTypeId::BOOL && right == SemanticTypeId::BOOL => {
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
            if left == right =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Add if left == SemanticTypeId::STRING || right == SemanticTypeId::STRING => {
            Ok(SemanticTypeId::STRING)
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if left == right && primitive_numeric(left) =>
        {
            Ok(left)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr
            if left == right && primitive_integer(left) =>
        {
            Ok(left)
        }
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

pub(super) fn semantic_type_for_local_path(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    let [segment] = path.segments.as_slice() else {
        return Err(SemanticError::unavailable("node_type"));
    };
    if !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("node_type"));
    }
    let declaration = resolve_lexical_declaration(program, index, reference, segment.node.name.node.name.as_str())
        .ok_or_else(|| SemanticError::unavailable("node_type"))?;
    local_declaration_type(program, index, declaration).unwrap_or_else(|| Err(SemanticError::unavailable("node_type")))
}

pub(super) fn local_declaration_type(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| semantic_type_from_syntax(&parameter.ty.node)),
        beskid_analysis::syntax_query::NodeKind::LambdaParameter => {
            index.node_at(program, parent)?.of::<beskid_analysis::syntax::LambdaParameter>().map(|parameter| {
                parameter.ty.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("node_type")),
                    |syntax_type| semantic_type_from_syntax(&syntax_type.node),
                )
            })
        }
        beskid_analysis::syntax_query::NodeKind::LetStatement => {
            index.node_at(program, parent)?.of::<beskid_analysis::syntax::LetStatement>().map(|statement| {
                statement.type_annotation.as_ref().map_or_else(
                    || semantic_type_for_expression(program, index, parent, &statement.value.node),
                    |syntax_type| semantic_type_from_syntax(&syntax_type.node),
                )
            })
        }
        beskid_analysis::syntax_query::NodeKind::ForStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::ForStatement>()
            .map(|statement| element_type_for_for_iterable(program, index, parent, &statement.iterable.node)),
        _ => None,
    }
}

pub(super) fn element_type_for_for_iterable(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    for_statement: beskid_analysis::syntax::AstNodeId,
    iterable: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    let beskid_analysis::syntax::Expression::Call(call) = iterable else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    let beskid_analysis::syntax::Expression::Path(path) = &call.node.callee.node else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    if segment.node.name.node.name != "range" || !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    }
    let [start, _end] = call.node.args.as_slice() else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    semantic_type_for_expression(program, index, for_statement, &start.node)
}

pub(super) fn semantic_type_for_literal(literal: &beskid_analysis::syntax::Literal) -> SemanticTypeId {
    match literal {
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i32") => SemanticTypeId::I32,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i64") => SemanticTypeId::I64,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_u8") => SemanticTypeId::U8,
        beskid_analysis::syntax::Literal::Integer(value)
            if value.starts_with("0x") && integer_literal_u64(value).is_some_and(|number| number > i64::MAX as u64) =>
        {
            SemanticTypeId::WORD
        }
        beskid_analysis::syntax::Literal::Integer(_) => SemanticTypeId::I32,
        beskid_analysis::syntax::Literal::Float(_) => SemanticTypeId::F64,
        beskid_analysis::syntax::Literal::String(_) => SemanticTypeId::STRING,
        beskid_analysis::syntax::Literal::Char(_) => SemanticTypeId::CHAR,
        beskid_analysis::syntax::Literal::Bool(_) => SemanticTypeId::BOOL,
    }
}
