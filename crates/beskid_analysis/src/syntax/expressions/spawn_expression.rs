use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `spawn` prefix expression: starts a new fiber from a callable operand.
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct SpawnExpression {
    #[ast(child)]
    pub callee: Box<Spanned<Expression>>,
}

pub(crate) fn parse_spawn_unary(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();
    let spawn_keyword = inner
        .next()
        .ok_or(ParseError::missing(Rule::SpawnKeyword))?;
    if spawn_keyword.as_rule() != Rule::SpawnKeyword {
        return Err(ParseError::unexpected_rule(spawn_keyword, Some(Rule::SpawnKeyword)));
    }
    let postfix = super::expression::parse_postfix_expression(
        inner
            .next()
            .ok_or(ParseError::missing(Rule::PostfixExpression))?,
    )?;
    Ok(Spanned::new(
        Expression::Spawn(Spanned::new(
            SpawnExpression {
                callee: Box::new(postfix),
            },
            span,
        )),
        span,
    ))
}
