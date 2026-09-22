use beskid_ast_derive::AstNode;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::{error::ParseError, parsable::Parsable};
use crate::syntax::{Block, LetStatement, SpanInfo, Spanned};

/// Lexically owned resource binding. The ordinary binding payload keeps initializer,
/// type, and local identity handling shared, while this node owns cleanup semantics.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopedUseStatement {
    #[ast(child)]
    pub binding: Spanned<LetStatement>,
    #[ast(child)]
    pub body: Option<Spanned<Block>>,
}

impl Parsable for ScopedUseStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let binding = inner.next().ok_or(ParseError::missing(Rule::ScopedUseBinding))?;
        let body = inner.next().map(Block::parse).transpose()?;
        Ok(Spanned::new(Self { binding: LetStatement::parse(binding)?, body }, span))
    }
}
