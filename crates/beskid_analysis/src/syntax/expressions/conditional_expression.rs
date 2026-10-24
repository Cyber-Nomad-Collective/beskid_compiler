use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::expressions::span::span_from_bounds;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `condition ? then_branch : else_branch`: the value of `then_branch` when `condition` is true,
/// otherwise the value of `else_branch`. Right-associative; binds looser than `||` and tighter
/// than assignment.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConditionalExpression {
    #[ast(child)]
    pub condition: Box<Spanned<Expression>>,
    #[ast(child)]
    pub then_branch: Box<Spanned<Expression>>,
    #[ast(child)]
    pub else_branch: Box<Spanned<Expression>>,
}

/// Build `condition ? then : else` from the condition and its `ConditionalTail` pair.
pub(crate) fn parse_conditional_tail(
    condition: Spanned<Expression>,
    tail: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let tail_end = tail.as_span().end();
    let input = tail.as_span().get_input();
    let mut inner = tail.into_inner();
    let then_branch = Expression::parse(inner.next().ok_or(ParseError::missing(Rule::Expression))?)?;
    let else_branch = Expression::parse(inner.next().ok_or(ParseError::missing(Rule::ConditionalElse))?)?;
    let span =
        span_from_bounds(input, condition.span.start, tail_end).ok_or(ParseError::missing(Rule::ConditionalTail))?;
    let conditional = Spanned::new(
        ConditionalExpression {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        },
        span,
    );
    Ok(Spanned::new(Expression::Conditional(conditional), span))
}

/// The else branch: an operand, or a nested conditional (right associativity).
pub(crate) fn parse_conditional_else(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();
    let condition = Expression::parse(inner.next().ok_or(ParseError::missing(Rule::LogicalOrExpression))?)?;
    match inner.next() {
        Some(tail) => parse_conditional_tail(condition, tail),
        None => Ok(Spanned::new(condition.node, span)),
    }
}
