use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::parse_doc_attached_list;
use crate::syntax::{Field, Identifier, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub fields: Vec<Spanned<Field>>,
    #[ast(skip)]
    pub field_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for EnumVariant {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let (fields, field_docs) = if let Some(field_list) = inner.next() {
            parse_doc_attached_list(field_list, Rule::FieldWithDocs, Rule::Field)?
        } else {
            (Vec::new(), Vec::new())
        };
        debug_assert_eq!(fields.len(), field_docs.len());

        Ok(Spanned::new(
            Self {
                name,
                fields,
                field_docs,
            },
            span,
        ))
    }
}
