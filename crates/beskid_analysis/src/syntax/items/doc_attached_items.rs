use pest::iterators::Pair;

use crate::doc::{leading_doc_from_doc_run, LeadingDocComment};
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::impl_block::ImplBlock;
use crate::syntax::{Node, Spanned};

pub fn parse_doc_attached_items<'i, I>(
    pairs: I,
) -> Result<(Vec<Spanned<Node>>, Vec<Option<LeadingDocComment>>), ParseError>
where
    I: IntoIterator<Item = Pair<'i, Rule>>,
{
    let mut items = Vec::new();
    let mut leading_docs = Vec::new();

    for item_with_docs in pairs {
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
            let methods = impl_block.node.methods;
            let method_docs = impl_block.node.method_docs;
            let mut impl_doc: Option<LeadingDocComment> = doc_opt;
            for (idx, method) in methods.into_iter().enumerate() {
                let mspan = method.span;
                items.push(Spanned::new(Node::Method(method), mspan));
                let method_doc = method_docs.get(idx).cloned().flatten();
                if idx == 0 {
                    leading_docs.push(method_doc.or_else(|| impl_doc.take()));
                } else {
                    leading_docs.push(method_doc);
                }
            }
            continue;
        }

        items.push(Node::parse(item_pair)?);
        leading_docs.push(doc_opt);
    }

    Ok((items, leading_docs))
}
