//! Semantic types for expressions, operands, locals, iterables, and literals.

use super::super::*;
use super::super::layouts::explicit_local_declaration_type;
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
    if let Some(moved) = node.of::<beskid_analysis::syntax::MoveExpression>() {
        return Some(semantic_type_for_expression(program, index, reference, &moved.operand.node));
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
        beskid_analysis::syntax::Expression::Move(moved) => {
            semantic_type_for_expression(program, index, reference, &moved.node.operand.node)
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

/// The syntax-only element type of a `for` iterable: the start of a `range(start, end)`, the
/// declared element of a local or parameter annotated `T[]`, or the first element of an array
/// literal. Iterables whose element type needs semantic facts (a call result, a field, a nominal
/// element) are the tracked `for_iterator_fact`'s, which proves them through source identity.
pub(in crate::semantic_contract) fn element_type_for_for_iterable(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    for_statement: beskid_analysis::syntax::AstNodeId,
    iterable: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    if let Some((start, _end)) = range_iterable_bounds(iterable) {
        return semantic_type_for_expression(program, index, for_statement, &start.node);
    }
    if let Some(element) = for_iterable_declared_array_element(program, index, for_statement, iterable) {
        return semantic_type_from_syntax(element);
    }
    if let beskid_analysis::syntax::Expression::ArrayLiteral(literal) = iterable
        && let Some(first) = literal.node.elements.first()
    {
        return semantic_type_for_expression(program, index, for_statement, &first.node);
    }
    Err(SemanticError::unavailable("for_iterator_element_type"))
}

/// The `(start, end)` operands of the syntax-only `range(start, end)` iterable form.
pub(in crate::semantic_contract) fn range_iterable_bounds(
    iterable: &beskid_analysis::syntax::Expression,
) -> Option<(
    &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
    &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
)> {
    let beskid_analysis::syntax::Expression::Call(call) = iterable else { return None };
    let beskid_analysis::syntax::Expression::Path(path) = &call.node.callee.node else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    if segment.node.name.node.name != "range" || !segment.node.type_args.is_empty() {
        return None;
    }
    let [start, end] = call.node.args.as_slice() else { return None };
    Some((start, end))
}

/// The declared element type of a `for` iterable that is a bare lexical local or parameter
/// annotated with an array type `T[]`.
pub(in crate::semantic_contract) fn for_iterable_declared_array_element<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    for_statement: beskid_analysis::syntax::AstNodeId,
    iterable: &beskid_analysis::syntax::Expression,
) -> Option<&'a beskid_analysis::syntax::Type> {
    let beskid_analysis::syntax::Expression::Path(path) = iterable else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, for_statement, segment.node.name.node.name.as_str())?;
    match explicit_local_declaration_type(program, index, declaration)? {
        beskid_analysis::syntax::Type::Array(element) => Some(&element.node),
        _ => None,
    }
}

/// The natural type of an integer literal, with no destination: its suffix, else `i32` when the
/// value fits, else `i64`, else `word` for a hexadecimal bit pattern above `i64::MAX`. The
/// syntax-layer [`integer_literal_primitive_type`](beskid_analysis::syntax::integer_literal_primitive_type)
/// is the sole authority, so a literal's type never claims a width that cannot hold its value.
pub(in crate::semantic_contract) fn integer_literal_semantic_type(text: &str) -> SemanticTypeId {
    semantic_type_for_primitive(beskid_analysis::syntax::integer_literal_primitive_type(text))
}

pub(in crate::semantic_contract) fn semantic_type_for_literal(
    literal: &beskid_analysis::syntax::Literal,
) -> SemanticTypeId {
    match literal {
        beskid_analysis::syntax::Literal::Integer(value) => integer_literal_semantic_type(value),
        beskid_analysis::syntax::Literal::Float(value) if value.ends_with("_f32") => SemanticTypeId::F32,
        beskid_analysis::syntax::Literal::Float(_) => SemanticTypeId::F64,
        beskid_analysis::syntax::Literal::String(_) => SemanticTypeId::STRING,
        beskid_analysis::syntax::Literal::Char(_) => SemanticTypeId::CHAR,
        beskid_analysis::syntax::Literal::Bool(_) => SemanticTypeId::BOOL,
        beskid_analysis::syntax::Literal::Unit => SemanticTypeId::UNIT,
    }
}
