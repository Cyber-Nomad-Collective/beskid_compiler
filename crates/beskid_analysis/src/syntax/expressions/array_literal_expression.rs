//! Array literal expression: `[elem0, elem1, ...]`.

use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `[elem0, elem1, ...]` — array literal expression.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArrayLiteralExpression {
    #[ast(children)]
    pub elements: Vec<Spanned<Expression>>,
}

pub(crate) fn parse_array_literal_expression(
    pair: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();

    let elements = if let Some(expr_list) = inner.next() {
        expr_list
            .into_inner()
            .map(Expression::parse)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    let literal = Spanned::new(ArrayLiteralExpression { elements }, span);

    Ok(Spanned::new(Expression::ArrayLiteral(literal), span))
}
