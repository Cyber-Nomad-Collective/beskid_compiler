use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::doc::LeadingDocComment;
use crate::syntax::items::parse_helpers::{
    parse_doc_attached_list, parse_identifier_list, parse_visibility_or_default,
};
use crate::syntax::{Field, Identifier, Path, SpanInfo, Spanned, Visibility};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct TypeDefinition {
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub generics: Vec<Spanned<Identifier>>,
    #[ast(children)]
    pub conformances: Vec<Spanned<Path>>,
    #[ast(children)]
    pub fields: Vec<Spanned<Field>>,
    #[ast(skip)]
    pub field_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for TypeDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut generics = Vec::new();
        let mut conformances = Vec::new();
        let mut fields = Vec::new();
        let mut field_docs = Vec::new();

        for item in inner {
            match item.as_rule() {
                Rule::GenericParameters => generics = parse_identifier_list(item)?,
                Rule::TypeConformanceList => {
                    let path_list = item
                        .into_inner()
                        .next()
                        .ok_or(ParseError::missing(Rule::PathList))?;
                    conformances = path_list
                        .into_inner()
                        .map(Path::parse)
                        .collect::<Result<Vec<_>, _>>()?
                }
                Rule::FieldList => {
                    let (parsed_fields, parsed_docs) =
                        parse_doc_attached_list(item, Rule::FieldWithDocs, Rule::Field)?;
                    fields = parsed_fields;
                    field_docs = parsed_docs;
                }
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }
        debug_assert_eq!(fields.len(), field_docs.len());

        Ok(Spanned::new(
            Self {
                visibility,
                name,
                generics,
                conformances,
                fields,
                field_docs,
            },
            span,
        ))
    }
}
