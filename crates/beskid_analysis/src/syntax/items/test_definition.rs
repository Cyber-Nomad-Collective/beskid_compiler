use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::{
    parse_attributes, parse_doc_attached_pair_raw, parse_visibility_or_default,
};
use crate::syntax::{
    Attribute, Expression, Identifier, SpanInfo, Spanned, Statement, Visibility,
};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TestMetadataEntry {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub value: Spanned<Expression>,
}

impl Parsable for TestMetadataEntry {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let value = Expression::parse(inner.next().ok_or(ParseError::missing(Rule::Expression))?)?;
        Ok(Spanned::new(Self { name, value }, span))
    }
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TestSkipEntry {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub value: Spanned<Expression>,
}

impl Parsable for TestSkipEntry {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let value = Expression::parse(inner.next().ok_or(ParseError::missing(Rule::Expression))?)?;
        Ok(Spanned::new(Self { name, value }, span))
    }
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TestMetaSection {
    #[ast(children)]
    pub entries: Vec<Spanned<TestMetadataEntry>>,
}

impl Parsable for TestMetaSection {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let entries = pair
            .into_inner()
            .filter(|inner| inner.as_rule() == Rule::TestMetadataEntry)
            .map(TestMetadataEntry::parse)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Spanned::new(Self { entries }, span))
    }
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TestSkipSection {
    #[ast(children)]
    pub entries: Vec<Spanned<TestSkipEntry>>,
}

impl Parsable for TestSkipSection {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let entries = pair
            .into_inner()
            .filter(|inner| inner.as_rule() == Rule::TestSkipEntry)
            .map(TestSkipEntry::parse)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Spanned::new(Self { entries }, span))
    }
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TestDefinition {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub meta: Option<Spanned<TestMetaSection>>,
    #[ast(child)]
    pub skip: Option<Spanned<TestSkipSection>>,
    #[ast(children)]
    pub statements: Vec<Spanned<Statement>>,
    #[ast(skip)]
    pub statement_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for TestDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let body = inner.next().ok_or(ParseError::missing(Rule::TestBody))?;

        let mut meta = None;
        let mut skip = None;
        let mut statements = Vec::new();
        let mut statement_docs = Vec::new();
        for body_item in body.into_inner() {
            let (doc_opt, item_pair) =
                parse_doc_attached_pair_raw(body_item, Rule::TestBodyItemWithDocs)?;
            match item_pair.as_rule() {
                Rule::TestMetaSection => {
                    meta = Some(TestMetaSection::parse(item_pair)?);
                }
                Rule::TestSkipSection => {
                    skip = Some(TestSkipSection::parse(item_pair)?);
                }
                _ => {
                    statements.push(Statement::parse(item_pair)?);
                    statement_docs.push(doc_opt);
                }
            }
        }
        debug_assert_eq!(statements.len(), statement_docs.len());

        Ok(Spanned::new(
            Self {
                attributes,
                visibility,
                name,
                meta,
                skip,
                statements,
                statement_docs,
            },
            span,
        ))
    }
}
