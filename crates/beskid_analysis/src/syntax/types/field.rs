use crate::syntax::{
    Attribute, Identifier, InjectQualifier, Parameter, PrimitiveType, Spanned, Type, Visibility,
};
use crate::syntax::items::parse_helpers::parse_attributes;

use beskid_ast_derive::AstNode;

/// Distinguishes ordinary value fields from event/signal-style fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FieldKind {
    Value,
    Event,
    Injected,
}

/// Struct or enum variant field with name and type (and optional event capacity).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Field {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(skip)]
    pub kind: FieldKind,
    #[ast(skip)]
    pub event_capacity: Option<usize>,
    #[ast(skip)]
    pub inject_qualifier: Option<InjectQualifier>,
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
        let mut field_inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut field_inner)?;
        let visibility = if field_inner
            .peek()
            .is_some_and(|item| item.as_rule() == crate::parser::Rule::Visibility)
        {
            <Visibility as crate::parsing::parsable::Parsable>::parse(field_inner.next().ok_or(
                crate::parsing::error::ParseError::missing(crate::parser::Rule::Visibility),
            )?)?
        } else {
            Spanned::new(Visibility::Private, span)
        };
        let field_node = field_inner
            .next()
            .ok_or(crate::parsing::error::ParseError::missing(
                crate::parser::Rule::ValueField,
            ))?;
        let (kind, mut inner) = match field_node.as_rule() {
            crate::parser::Rule::ValueField => (FieldKind::Value, field_node.into_inner()),
            crate::parser::Rule::EventField => (FieldKind::Event, field_node.into_inner()),
            crate::parser::Rule::InjectField => (FieldKind::Injected, field_node.into_inner()),
            _ => {
                return Err(crate::parsing::error::ParseError::unexpected_rule(
                    field_node,
                    Some(crate::parser::Rule::Field),
                ));
            }
        };
        let (event_capacity, inject_qualifier, name, ty) = match kind {
            FieldKind::Value => {
                let ty = crate::syntax::Type::parse(inner.next().ok_or(
                    crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType),
                )?)?;
                let name = crate::syntax::Identifier::parse(inner.next().ok_or(
                    crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier),
                )?)?;
                (None, None, name, ty)
            }
            FieldKind::Event => {
                let first = inner
                    .next()
                    .ok_or(crate::parsing::error::ParseError::missing(
                        crate::parser::Rule::Identifier,
                    ))?;
                let (event_capacity, name_pair) =
                    if first.as_rule() == crate::parser::Rule::EventCapacity {
                        let mut cap_inner = first.into_inner();
                        let value =
                            cap_inner
                                .next()
                                .ok_or(crate::parsing::error::ParseError::missing(
                                    crate::parser::Rule::IntegerLiteral,
                                ))?;
                        let parsed = value.as_str().parse::<usize>().map_err(|_| {
                            crate::parsing::error::ParseError::missing(
                                crate::parser::Rule::IntegerLiteral,
                            )
                        })?;
                        let name_pair =
                            inner
                                .next()
                                .ok_or(crate::parsing::error::ParseError::missing(
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
                        .filter_map(|entry| {
                            if entry.as_rule() == crate::parser::Rule::ParameterWithDocs {
                                Some(entry)
                            } else {
                                None
                            }
                        })
                        .map(|entry| {
                            let mut inner = entry.into_inner();
                            let first =
                                inner
                                    .next()
                                    .ok_or(crate::parsing::error::ParseError::missing(
                                        crate::parser::Rule::Parameter,
                                    ))?;
                            let parameter_pair = if first.as_rule() == crate::parser::Rule::DocRun {
                                inner
                                    .next()
                                    .ok_or(crate::parsing::error::ParseError::missing(
                                        crate::parser::Rule::Parameter,
                                    ))?
                            } else {
                                first
                            };
                            Parameter::parse(parameter_pair)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    Vec::new()
                };

                let return_type = Spanned::new(
                    Type::Primitive(Spanned::new(PrimitiveType::Unit, span)),
                    span,
                );
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
                (event_capacity, None, name, ty)
            }
            FieldKind::Injected => {
                let first = inner
                    .next()
                    .ok_or(crate::parsing::error::ParseError::missing(
                        crate::parser::Rule::BeskidType,
                    ))?;
                let (inject_qualifier, ty_pair) =
                    if first.as_rule() == crate::parser::Rule::InjectQualifier {
                        let qualifier = match first.as_str().strip_suffix("::") {
                            Some("global") => Some(InjectQualifier::Global),
                            Some("parent") => Some(InjectQualifier::Parent),
                            _ => {
                                return Err(crate::parsing::error::ParseError::unexpected_rule(
                                    first,
                                    Some(crate::parser::Rule::InjectQualifier),
                                ));
                            }
                        };
                        (
                            qualifier,
                            inner
                                .next()
                                .ok_or(crate::parsing::error::ParseError::missing(
                                    crate::parser::Rule::BeskidType,
                                ))?,
                        )
                    } else {
                        (None, first)
                    };
                let ty = crate::syntax::Type::parse(ty_pair)?;
                let name = crate::syntax::Identifier::parse(inner.next().ok_or(
                    crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier),
                )?)?;
                (None, inject_qualifier, name, ty)
            }
        };

        Ok(crate::syntax::Spanned::new(
            Self {
                attributes,
                visibility,
                kind,
                event_capacity,
                inject_qualifier,
                name,
                ty,
            },
            span,
        ))
    }
}
