//! The branch that types a conditional expression `cond ? a : b`.
//!
//! Both branches have one type, which is the conditional's type. A bare numeric literal branch
//! (an unsuffixed or suffixed integer or float literal, possibly negated, possibly parenthesized)
//! has no type of its own: it takes the other branch's type (`c ? 1 : 2_i64` is `i64`). When
//! both branches are literals, the then-branch's default type decides. So the result branch is
//! the then-branch unless only the then-branch is a literal.

use super::super::*;
use beskid_analysis::syntax::{ConditionalExpression, Expression, Literal, Spanned, UnaryOp};

/// Whether `expression` is a bare numeric literal branch, through grouping and negation.
pub(in crate::semantic_contract) fn is_numeric_literal_branch(expression: &Expression) -> bool {
    match expression {
        Expression::Literal(literal) => matches!(literal.node.literal.node, Literal::Integer(_) | Literal::Float(_)),
        Expression::Grouped(grouped) => is_numeric_literal_branch(&grouped.node.expr.node),
        Expression::Unary(unary) => {
            matches!(unary.node.op.node, UnaryOp::Neg) && is_numeric_literal_branch(&unary.node.expr.node)
        }
        _ => false,
    }
}

/// The branch whose type is the conditional's type.
pub(in crate::semantic_contract) fn conditional_result_branch(
    conditional: &ConditionalExpression,
) -> &Spanned<Expression> {
    if is_numeric_literal_branch(&conditional.then_branch.node) && !is_numeric_literal_branch(&conditional.else_branch.node) {
        conditional.else_branch.as_ref()
    } else {
        conditional.then_branch.as_ref()
    }
}

/// The direct child node of the conditional at `node` holding its result branch (before
/// normalization), when the index has it.
fn conditional_result_branch_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
    conditional: &ConditionalExpression,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let branch = conditional_result_branch(conditional);
    index.direct_child_id(program, node, beskid_analysis::syntax_query::DynNodeRef::from(branch))
}

/// The result branch of the conditional expression at `key`, normalized to its value node.
/// `None` when `key` is not a conditional expression.
pub(in crate::semantic_contract) fn conditional_result_branch_key(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<AstNodeKey> {
    let conditional = index.node_at(program, key.node)?.of::<ConditionalExpression>()?;
    let branch = conditional_result_branch_node(program, index, key.node, conditional)?;
    Some(AstNodeKey { node: normalized_expression_node(index, branch), ..key })
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn conditional_result_branch_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, _node| conditional_result_branch_key(program, index, key))
}
