use pest::iterators::{Pair, Pairs};
use std::iter::Peekable;

use crate::doc::{LeadingDocComment, leading_doc_from_doc_run};
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Attribute, Identifier, Parameter, SpanInfo, Spanned, Visibility};

pub(crate) fn parse_attributes(
    inner: &mut Peekable<Pairs<Rule>>,
) -> Result<Vec<Spanned<Attribute>>, ParseError> {
    if let Some(next) = inner.peek()
        && next.as_rule() == Rule::AttributeList
    {
        return inner
            .next()
            .expect("attribute list pair")
            .into_inner()
            .map(Attribute::parse)
            .collect();
    }
    Ok(Vec::new())
}

pub(crate) fn parse_visibility_or_default(
    pair: &Pair<Rule>,
    inner: &mut Peekable<Pairs<Rule>>,
) -> Result<Spanned<Visibility>, ParseError> {
    if let Some(next) = inner.peek()
        && next.as_rule() == Rule::Visibility
    {
        return Visibility::parse(inner.next().expect("visibility pair"));
    }

    Ok(Spanned::new(
        Visibility::Private,
        SpanInfo::from_span(&pair.as_span()),
    ))
}

pub(crate) fn parse_identifier_list(
    pair: Pair<Rule>,
) -> Result<Vec<Spanned<Identifier>>, ParseError> {
    pair.into_inner().map(Identifier::parse).collect()
}

pub(crate) fn parse_parameter_list_with_docs(
    pair: Pair<Rule>,
) -> Result<(Vec<Spanned<Parameter>>, Vec<Option<LeadingDocComment>>), ParseError> {
    parse_doc_attached_list(pair, Rule::ParameterWithDocs, Rule::Parameter)
}

pub(crate) fn parse_doc_attached_pair<T>(
    pair: Pair<Rule>,
    wrapper_rule: Rule,
    inner_rule: Rule,
) -> Result<(Option<LeadingDocComment>, Spanned<T>), ParseError>
where
    T: Parsable,
{
    if pair.as_rule() != wrapper_rule {
        return Err(ParseError::unexpected_rule(pair, Some(wrapper_rule)));
    }
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ParseError::missing(wrapper_rule))?;
    let (doc_opt, value_pair) = if first.as_rule() == Rule::DocRun {
        let doc = leading_doc_from_doc_run(&first);
        let next = inner.next().ok_or(ParseError::missing(inner_rule))?;
        (Some(doc), next)
    } else {
        (None, first)
    };
    if value_pair.as_rule() != inner_rule {
        return Err(ParseError::unexpected_rule(value_pair, Some(inner_rule)));
    }
    Ok((doc_opt, T::parse(value_pair)?))
}

pub(crate) fn parse_doc_attached_with<T, F>(
    pair: Pair<Rule>,
    wrapper_rule: Rule,
    parse_inner: F,
) -> Result<(Option<LeadingDocComment>, Spanned<T>), ParseError>
where
    F: FnOnce(Pair<Rule>) -> Result<Spanned<T>, ParseError>,
{
    if pair.as_rule() != wrapper_rule {
        return Err(ParseError::unexpected_rule(pair, Some(wrapper_rule)));
    }
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ParseError::missing(wrapper_rule))?;
    let (doc_opt, value_pair) = if first.as_rule() == Rule::DocRun {
        let doc = leading_doc_from_doc_run(&first);
        let next = inner.next().ok_or(ParseError::missing(wrapper_rule))?;
        (Some(doc), next)
    } else {
        (None, first)
    };
    Ok((doc_opt, parse_inner(value_pair)?))
}

pub(crate) fn parse_doc_attached_pair_raw(
    pair: Pair<Rule>,
    wrapper_rule: Rule,
) -> Result<(Option<LeadingDocComment>, Pair<Rule>), ParseError> {
    if pair.as_rule() != wrapper_rule {
        return Err(ParseError::unexpected_rule(pair, Some(wrapper_rule)));
    }
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ParseError::missing(wrapper_rule))?;
    let (doc_opt, value_pair) = if first.as_rule() == Rule::DocRun {
        let doc = leading_doc_from_doc_run(&first);
        let next = inner.next().ok_or(ParseError::missing(wrapper_rule))?;
        (Some(doc), next)
    } else {
        (None, first)
    };
    Ok((doc_opt, value_pair))
}

pub(crate) fn parse_doc_attached_list<T>(
    pair: Pair<Rule>,
    wrapper_rule: Rule,
    inner_rule: Rule,
) -> Result<(Vec<Spanned<T>>, Vec<Option<LeadingDocComment>>), ParseError>
where
    T: Parsable,
{
    let mut values = Vec::new();
    let mut docs = Vec::new();
    for inner in pair.into_inner().filter(|inner| inner.as_rule() == wrapper_rule) {
        let (doc, value) = parse_doc_attached_pair(inner, wrapper_rule, inner_rule)?;
        values.push(value);
        docs.push(doc);
    }
    Ok((values, docs))
}
