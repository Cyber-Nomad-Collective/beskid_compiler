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

/// Accessor requirements occupy ordinary typed witness slots. Their reserved names
/// cannot collide with a method declared in source.
pub(super) fn property_signatures(pair: Pair<Rule>) -> Result<Vec<Spanned<ContractMethodSignature>>, ParseError> {
    use crate::syntax::{PrimitiveType, PropertyAccessorKind};
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner().peekable();
    let attributes = parse_attributes(&mut inner)?;
    let ty = Type::parse(inner.next().ok_or(ParseError::missing(Rule::BeskidType))?)?;
    let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
    inner
        .map(|accessor| {
            let kind = match accessor.as_rule() {
                Rule::ContractGet => PropertyAccessorKind::Get,
                Rule::ContractSet => PropertyAccessorKind::Set,
                _ => return Err(ParseError::unexpected_rule(accessor, None)),
            };
            let at = SpanInfo::from_span(&accessor.as_span());
            let (parameters, result) = match kind {
                PropertyAccessorKind::Get => (Vec::new(), ty.clone()),
                PropertyAccessorKind::Set => {
                    let value = Parameter {
                        attributes: Vec::new(),
                        bulk: false,
                        mutable: false,
                        movable: false,
                        name: Spanned::new(Identifier { name: "value".into() }, at),
                        ty: ty.clone(),
                        default: None,
                    };
                    (
                        vec![Spanned::new(value, at)],
                        Spanned::new(Type::Primitive(Spanned::new(PrimitiveType::Unit, at)), at),
                    )
                }
            };
            Ok(Spanned::new(
                ContractMethodSignature {
                    attributes: attributes.clone(),
                    name: Spanned::new(Identifier { name: kind.method_name(&name.node.name) }, name.span),
                    parameter_docs: vec![None; parameters.len()],
                    parameters,
                    return_type: Some(result),
                },
                span,
            ))
        })
        .collect()
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
