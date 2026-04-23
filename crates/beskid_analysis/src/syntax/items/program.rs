use pest::iterators::Pair;

use crate::doc::{leading_doc_from_doc_run, LeadingDocComment};
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::impl_block::ImplBlock;
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
        let mut items = Vec::new();
        let mut leading_docs: Vec<Option<LeadingDocComment>> = Vec::new();

        for item_with_docs in pair.into_inner().filter(|p| p.as_rule() != Rule::EOI) {
            if item_with_docs.as_rule() != Rule::ItemWithDocs {
                return Err(ParseError::unexpected_rule(
                    item_with_docs,
                    Some(Rule::ItemWithDocs),
                ));
            }

            let mut inner = item_with_docs.into_inner();
            let first = inner
                .next()
                .ok_or_else(|| ParseError::missing(Rule::ItemWithDocs))?;
            let (doc_opt, item_pair) = if first.as_rule() == Rule::DocRun {
                let d = leading_doc_from_doc_run(&first);
                let itemp = inner
                    .next()
                    .ok_or_else(|| ParseError::missing(Rule::InnerItem))?;
                (Some(d), itemp)
            } else {
                (None, first)
            };

            if item_pair.as_rule() == Rule::ImplBlock {
                let impl_block = ImplBlock::parse(item_pair)?;
                let mut first_doc = doc_opt;
                for method in impl_block.node.methods {
                    let mspan = method.span;
                    items.push(Spanned::new(Node::Method(method), mspan));
                    leading_docs.push(first_doc.take());
                }
                continue;
            }

            items.push(Node::parse(item_pair)?);
            leading_docs.push(doc_opt);
        }

        Ok(Spanned::new(
            Self {
                items,
                leading_docs,
            },
            span,
        ))
    }
}
