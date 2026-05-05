use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::{
    parse_attributes, parse_doc_attached_with, parse_visibility_or_default,
};
use crate::syntax::{Attribute, ContractNode, Identifier, SpanInfo, Spanned, Visibility};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct ContractDefinition {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub items: Vec<Spanned<ContractNode>>,
    #[ast(skip)]
    pub item_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for ContractDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let mut items = Vec::new();
        let mut item_docs = Vec::new();
        for pair in inner {
            let (doc, item) = parse_doc_attached_with(pair, Rule::ContractItemWithDocs, |inner_pair| {
                ContractNode::parse(inner_pair)
            })?;
            items.push(item);
            item_docs.push(doc);
        }
        debug_assert_eq!(items.len(), item_docs.len());

        Ok(Spanned::new(
            Self {
                attributes,
                visibility,
                name,
                items,
                item_docs,
            },
            span,
        ))
    }
}
