use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Block, Expression, Identifier, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct ForStatement {
    #[ast(child)]
    pub iterator: Spanned<Identifier>,
    #[ast(child)]
    pub iterable: Spanned<Expression>,
    #[ast(child)]
    pub body: Spanned<Block>,
}

impl Parsable for ForStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let iterator =
            Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let iterable =
            Expression::parse(inner.next().ok_or(ParseError::missing(Rule::Expression))?)?;
        let body = Block::parse(inner.next().ok_or(ParseError::missing(Rule::Block))?)?;

        Ok(Spanned::new(
            Self {
                iterator,
                iterable,
                body,
            },
            span,
        ))
    }
}
