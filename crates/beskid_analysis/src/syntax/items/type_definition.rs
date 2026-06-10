use pest::iterators::Pair;

use crate::doc::LeadingDocComment;
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::method_definition::MethodDefinition;
use crate::syntax::items::parse_helpers::{
    parse_attributes, parse_doc_attached_list, parse_doc_attached_with, parse_identifier_list,
    parse_visibility_or_default,
};
use crate::syntax::{
    Attribute, Field, Identifier, Path, PathSegment, SpanInfo, Spanned, Type, Visibility,
};

use beskid_ast_derive::AstNode;

/// `type` definition: name, generics, optional conformances, fields, and inline methods.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TypeDefinition {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
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
    #[ast(children)]
    pub methods: Vec<Spanned<MethodDefinition>>,
    #[ast(skip)]
    pub method_docs: Vec<Option<LeadingDocComment>>,
}

fn receiver_type_for_definition(
    name: &Spanned<Identifier>,
    generics: &[Spanned<Identifier>],
    span: SpanInfo,
) -> Spanned<Type> {
    let type_args = generics
        .iter()
        .map(|generic| {
            Spanned::new(
                Type::Complex(Spanned::new(
                    Path {
                        segments: vec![Spanned::new(
                            PathSegment {
                                name: generic.clone(),
                                type_args: Vec::new(),
                            },
                            generic.span,
                        )],
                    },
                    generic.span,
                )),
                generic.span,
            )
        })
        .collect();
    Spanned::new(
        Type::Complex(Spanned::new(
            Path {
                segments: vec![Spanned::new(
                    PathSegment {
                        name: name.clone(),
                        type_args,
                    },
                    name.span,
                )],
            },
            span,
        )),
        span,
    )
}

impl Parsable for TypeDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut generics = Vec::new();
        let mut conformances = Vec::new();
        let mut fields = Vec::new();
        let mut field_docs = Vec::new();
        let mut methods = Vec::new();
        let mut method_docs = Vec::new();

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
                Rule::TypeBody => {
                    let receiver_type = receiver_type_for_definition(&name, &generics, span);
                    for member in item.into_inner() {
                        match member.as_rule() {
                            Rule::TypeFieldList => {
                                let (parsed_fields, parsed_docs) = parse_doc_attached_list(
                                    member,
                                    Rule::FieldWithDocs,
                                    Rule::Field,
                                )?;
                                fields = parsed_fields;
                                field_docs = parsed_docs;
                            }
                            Rule::ImplMethodWithDocs => {
                                let (doc_opt, method) = parse_doc_attached_with(
                                    member,
                                    Rule::ImplMethodWithDocs,
                                    |inner_pair| {
                                        MethodDefinition::parse_with_receiver(
                                            inner_pair,
                                            receiver_type.clone(),
                                        )
                                    },
                                )?;
                                methods.push(method);
                                method_docs.push(doc_opt);
                            }
                            _ => return Err(ParseError::unexpected_rule(member, None)),
                        }
                    }
                }
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }
        debug_assert_eq!(fields.len(), field_docs.len());
        debug_assert_eq!(methods.len(), method_docs.len());

        Ok(Spanned::new(
            Self {
                attributes,
                visibility,
                name,
                generics,
                conformances,
                fields,
                field_docs,
                methods,
                method_docs,
            },
            span,
        ))
    }
}
