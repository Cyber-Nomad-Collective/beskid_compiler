use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// `break` out of the nearest enclosing loop.
#[derive(AstNode, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BreakStatement;

impl Parsable for BreakStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        Ok(Spanned::new(Self, span))
    }
}
