use crate::syntax::{Identifier, Parameter, PrimitiveType, Spanned, Type};

use beskid_ast_derive::AstNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Value,
    Event,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct Field {
    #[ast(skip)]
    pub kind: FieldKind,
    #[ast(skip)]
    pub event_capacity: Option<usize>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub ty: Spanned<Type>,
}

impl crate::parsing::parsable::Parsable for Field {
    fn parse(
        pair: pest::iterators::Pair<crate::parser::Rule>,
    ) -> Result<crate::syntax::Spanned<Self>, crate::parsing::error::ParseError> {
        if pair.as_rule() != crate::parser::Rule::Field {
            return Err(crate::parsing::error::ParseError::unexpected_rule(
                pair,
                Some(crate::parser::Rule::Field),
            ));
        }

        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());
        let field_node = pair
            .into_inner()
            .next()
            .ok_or(crate::parsing::error::ParseError::missing(
                crate::parser::Rule::ValueField,
            ))?;
        let (kind, mut inner) = match field_node.as_rule() {
            crate::parser::Rule::ValueField => (FieldKind::Value, field_node.into_inner()),
            crate::parser::Rule::EventField => (FieldKind::Event, field_node.into_inner()),
            _ => {
                return Err(crate::parsing::error::ParseError::unexpected_rule(
                    field_node,
                    Some(crate::parser::Rule::Field),
                ));
            }
        };
        let (event_capacity, name, ty) = match kind {
            FieldKind::Value => {
                let ty = crate::syntax::Type::parse(inner.next().ok_or(
                    crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType),
                )?)?;
                let name = crate::syntax::Identifier::parse(inner.next().ok_or(
                    crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier),
                )?)?;
                (None, name, ty)
            }
            FieldKind::Event => {
                let first = inner.next().ok_or(crate::parsing::error::ParseError::missing(
                    crate::parser::Rule::Identifier,
                ))?;
                let (event_capacity, name_pair) = if first.as_rule() == crate::parser::Rule::EventCapacity {
                    let mut cap_inner = first.into_inner();
                    let value = cap_inner.next().ok_or(crate::parsing::error::ParseError::missing(
                        crate::parser::Rule::IntegerLiteral,
                    ))?;
                    let parsed = value
                        .as_str()
                        .parse::<usize>()
                        .map_err(|_| crate::parsing::error::ParseError::missing(crate::parser::Rule::IntegerLiteral))?;
                    let name_pair = inner.next().ok_or(crate::parsing::error::ParseError::missing(
                        crate::parser::Rule::Identifier,
                    ))?;
                    (Some(parsed), name_pair)
                } else {
                    (None, first)
                };

                let name = crate::syntax::Identifier::parse(name_pair)?;
                let params_pair = inner.next();
                let params = if let Some(pair) = params_pair {
                    pair.into_inner()
                        .map(Parameter::parse)
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    Vec::new()
                };

                let return_type = Spanned::new(Type::Primitive(Spanned::new(PrimitiveType::Unit, span)), span);
                let parameter_types = params
                    .into_iter()
                    .map(|param| param.node.ty)
                    .collect::<Vec<_>>();
                let ty = Spanned::new(
                    Type::Function {
                        return_type: Box::new(return_type),
                        parameters: parameter_types,
                    },
                    span,
                );
                (event_capacity, name, ty)
            }
        };

        Ok(crate::syntax::Spanned::new(
            Self {
                kind,
                event_capacity,
                name,
                ty,
            },
            span,
        ))
    }
}
