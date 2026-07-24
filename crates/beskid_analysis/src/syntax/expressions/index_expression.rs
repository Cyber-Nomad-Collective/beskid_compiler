//! Postfix subscript operator: `expr[index]`.

use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `expr[index]` — array/string element access.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndexExpression {
    #[ast(child)]
    pub target: Box<Spanned<Expression>>,
    #[ast(child)]
    pub index: Box<Spanned<Expression>>,
}

pub(crate) fn parse_index_expression(
    target: Spanned<Expression>,
    pair: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let index_pair = pair.into_inner().next().ok_or(ParseError::missing(Rule::Expression))?;
    let index = Expression::parse(index_pair)?;

    let index_expr = Spanned::new(IndexExpression { target: Box::new(target), index: Box::new(index) }, span);

    Ok(Spanned::new(Expression::Index(index_expr), span))
}
