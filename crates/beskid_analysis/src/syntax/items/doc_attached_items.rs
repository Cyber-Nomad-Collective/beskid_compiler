use pest::iterators::Pair;

use crate::doc::{LeadingDocComment, leading_doc_from_doc_run};
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Node, Spanned};

/// Parses `ItemWithDocs` pairs into nodes and parallel leading-doc slots.
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
            return Err(ParseError::unexpected_rule(item_with_docs, Some(Rule::ItemWithDocs)));
        }

        let mut inner = item_with_docs.into_inner();
        let first = inner.next().ok_or_else(|| ParseError::missing(Rule::ItemWithDocs))?;
        let (doc_opt, item_pair) = if first.as_rule() == Rule::DocRun {
            let d = leading_doc_from_doc_run(&first);
            let itemp = inner.next().ok_or_else(|| ParseError::missing(Rule::InnerItem))?;
            (Some(d), itemp)
        } else {
            (None, first)
        };

        items.push(Node::parse(item_pair)?);
        leading_docs.push(doc_opt);
    }

    Ok((items, leading_docs))
}
