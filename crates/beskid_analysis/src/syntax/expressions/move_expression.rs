use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `move` prefix expression: consumes an owning name (`move x`) or destructively reads a field
/// (`move x.f`). The operand is a postfix expression; the semantic layer admits only paths.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MoveExpression {
    #[ast(child)]
    pub operand: Box<Spanned<Expression>>,
}

pub(crate) fn parse_move_expression(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();
    let move_keyword = inner.next().ok_or(ParseError::missing(Rule::MoveKeyword))?;
    if move_keyword.as_rule() != Rule::MoveKeyword {
        return Err(ParseError::unexpected_rule(move_keyword, Some(Rule::MoveKeyword)));
    }
    let operand =
        super::expression::parse_postfix_expression(inner.next().ok_or(ParseError::missing(Rule::PostfixExpression))?)?;
    Ok(Spanned::new(Expression::Move(Spanned::new(MoveExpression { operand: Box::new(operand) }, span)), span))
}
