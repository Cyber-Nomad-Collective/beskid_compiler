use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// Statement that evaluates an expression for side effects (typically terminated with `;`).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpressionStatement {
    #[ast(child)]
    pub expression: Spanned<Expression>,
}

impl Parsable for ExpressionStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let expression = Expression::parse(pair.into_inner().next().ok_or(ParseError::missing(Rule::Expression))?)?;

        Ok(Spanned::new(Self { expression }, span))
    }
}
