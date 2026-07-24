use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::{parse_attributes, parse_parameter_list_with_docs};
use crate::syntax::{Attribute, Identifier, Parameter, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Abstract method signature inside a `contract` (no body).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractMethodSignature {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
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
        let mut inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let return_type = Some(Type::parse(inner.next().ok_or(ParseError::missing(Rule::BeskidType))?)?);
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

        Ok(Spanned::new(Self { attributes, name, parameters, parameter_docs, return_type }, span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{BeskidParser, Rule};
    use pest::Parser;

    #[test]
    fn contract_method_signature_parses_leading_attributes() {
        let source = "[Export] ListStep Push(T value);";
        let pair = BeskidParser::parse(Rule::ContractMethodSignature, source).expect("parse").next().expect("pair");
        let signature = ContractMethodSignature::parse(pair).expect("signature");
        assert_eq!(signature.node.attributes.len(), 1);
        assert_eq!(signature.node.attributes[0].node.name.node.name, "Export");
    }
}
