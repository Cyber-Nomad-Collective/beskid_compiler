//! Semantic types for expressions, operands, locals, iterables, and literals.

use super::super::*;
use super::*;

pub(in crate::semantic_contract) fn semantic_type_for_node(
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
    if let Some(unary) = node.of::<beskid_analysis::syntax::UnaryExpression>() {
        return Some(semantic_type_for_unary_operand(program, index, reference, unary));
    }
    if let Some(match_expression) = node.of::<beskid_analysis::syntax::MatchExpression>() {
        let mut result = None;
        for arm in &match_expression.arms {
            let arm_type = match semantic_type_for_expression(program, index, reference, &arm.node.value.node) {
                Ok(arm_type) => arm_type,
                Err(error) => return Some(Err(error)),
            };
            result = match join_match_arm_type(result, arm_type) {
                Ok(result) => result,
                Err(error) => return Some(Err(error)),
            };
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

pub(in crate::semantic_contract) fn semantic_type_for_expression(
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
        beskid_analysis::syntax::Expression::Unary(unary) => {
            semantic_type_for_unary_operand(program, index, reference, &unary.node)
        }
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

fn semantic_type_for_unary_operand(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    unary: &beskid_analysis::syntax::UnaryExpression,
) -> Result<SemanticTypeId, SemanticError> {
    let operand = semantic_type_for_expression(program, index, reference, &unary.expr.node)?;
    match unary.op.node {
        beskid_analysis::syntax::UnaryOp::Neg if primitive_numeric(operand) => Ok(operand),
        beskid_analysis::syntax::UnaryOp::Not if operand == SemanticTypeId::BOOL => Ok(operand),
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

pub(in crate::semantic_contract) fn semantic_type_for_binary_operands(
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

pub(in crate::semantic_contract) fn semantic_type_for_local_path(
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

pub(in crate::semantic_contract) fn local_declaration_type(
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

pub(in crate::semantic_contract) fn element_type_for_for_iterable(
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

pub(in crate::semantic_contract) fn semantic_type_for_literal(
    literal: &beskid_analysis::syntax::Literal,
) -> SemanticTypeId {
    match literal {
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i32") => SemanticTypeId::I32,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i64") => SemanticTypeId::I64,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_u32") => SemanticTypeId::U32,
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
        beskid_analysis::syntax::Literal::Unit => SemanticTypeId::UNIT,
    }
}
