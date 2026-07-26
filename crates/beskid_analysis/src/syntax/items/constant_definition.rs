use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Identifier, Literal, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// Module-scoped integer constant. Its literal initializer is the sole value authority.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConstantDefinition {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub value: Spanned<Literal>,
}

impl Parsable for ConstantDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let value = Literal::parse(inner.next().ok_or(ParseError::missing(Rule::IntegerLiteral))?)?;
        Ok(Spanned::new(Self { name, value }, span))
    }
}
