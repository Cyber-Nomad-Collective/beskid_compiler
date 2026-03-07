use crate::syntax::{Identifier, Spanned, Type};

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
        let ty = crate::syntax::Type::parse(inner.next().ok_or(
            crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType),
        )?)?;
        let name = crate::syntax::Identifier::parse(inner.next().ok_or(
            crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier),
        )?)?;

        Ok(crate::syntax::Spanned::new(Self { kind, name, ty }, span))
    }
}
