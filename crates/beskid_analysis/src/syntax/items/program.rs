use pest::iterators::Pair;

use crate::doc::LeadingDocComment;
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::doc_attached_items::parse_doc_attached_items;
use crate::syntax::{Node, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct Program {
    #[ast(children)]
    pub items: Vec<Spanned<Node>>,
    #[ast(skip)]
    pub leading_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for Program {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let (items, leading_docs) = if let Some(item_list) =
            pair.into_inner().find(|p| p.as_rule() == Rule::ItemList)
        {
            parse_doc_attached_items(item_list.into_inner().filter(|p| p.as_rule() == Rule::ItemWithDocs))?
        } else {
            (Vec::new(), Vec::new())
        };

        Ok(Spanned::new(
            Self {
                items,
                leading_docs,
            },
            span,
        ))
    }
}
