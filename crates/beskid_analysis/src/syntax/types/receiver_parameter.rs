use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::parse_attributes;
use crate::syntax::{Attribute, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// Explicit method receiver (`this`, `[A] this`, `mut this`, `move this`) written as the first
/// entry of a method parameter list. A method without one has an implicit borrowed receiver.
/// `move this` declares an owning receiver: the call consumes the receiver value.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReceiverParameter {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(skip)]
    pub mutable: bool,
    #[ast(skip)]
    pub movable: bool,
}

impl Parsable for ReceiverParameter {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        if pair.as_rule() != Rule::ReceiverParameter {
            return Err(ParseError::unexpected_rule(pair, Some(Rule::ReceiverParameter)));
        }
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let mut mutable = false;
        let mut movable = false;
        for item in inner {
            match item.as_rule() {
                Rule::MutKeyword => mutable = true,
                Rule::MoveKeyword => movable = true,
                _ => return Err(ParseError::unexpected_rule(item, Some(Rule::ReceiverParameter))),
            }
        }
        Ok(Spanned::new(Self { attributes, mutable, movable }, span))
    }
}
