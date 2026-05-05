use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::parse_parameter_list_with_docs;
use crate::syntax::{Identifier, Parameter, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct ContractMethodSignature {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub parameters: Vec<Spanned<Parameter>>,
    #[ast(skip)]
    pub parameter_docs: Vec<Option<LeadingDocComment>>,
    #[ast(child)]
    pub return_type: Option<Spanned<Type>>,
}

impl Parsable for ContractMethodSignature {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let return_type = Some(Type::parse(
            inner.next().ok_or(ParseError::missing(Rule::BeskidType))?,
        )?);
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let mut parameters = Vec::new();
        let mut parameter_docs = Vec::new();

        for item in inner {
            match item.as_rule() {
                Rule::ParameterList => {
                    let (parsed_parameters, parsed_docs) = parse_parameter_list_with_docs(item)?;
                    parameters = parsed_parameters;
                    parameter_docs = parsed_docs;
                }
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }
        debug_assert_eq!(parameters.len(), parameter_docs.len());

        Ok(Spanned::new(
            Self {
                name,
                parameters,
                parameter_docs,
                return_type,
            },
            span,
        ))
    }
}
