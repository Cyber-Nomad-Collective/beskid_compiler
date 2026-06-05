use crate::syntax::{Identifier, ParameterModifier, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Function or method parameter: optional modifier, name, and type (`ty name` surface order).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Parameter {
    #[ast(child)]
    pub modifier: Option<Spanned<ParameterModifier>>,
    #[ast(skip)]
    pub mutable: bool,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub ty: Spanned<Type>,
}

impl crate::parsing::parsable::Parsable for Parameter {
    fn parse(
        pair: pest::iterators::Pair<crate::parser::Rule>,
    ) -> Result<crate::syntax::Spanned<Self>, crate::parsing::error::ParseError> {
        if pair.as_rule() != crate::parser::Rule::Parameter {
            return Err(crate::parsing::error::ParseError::unexpected_rule(
                pair,
                Some(crate::parser::Rule::Parameter),
            ));
        }

        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let first = inner
            .next()
            .ok_or(crate::parsing::error::ParseError::missing(
                crate::parser::Rule::BeskidType,
            ))?;

        let (modifier, ty_pair) = if first.as_rule() == crate::parser::Rule::ParameterModifier {
            let modifier = crate::syntax::ParameterModifier::parse(first)?;
            let ty_pair = inner
                .next()
                .ok_or(crate::parsing::error::ParseError::missing(
                    crate::parser::Rule::BeskidType,
                ))?;
            (Some(modifier), ty_pair)
        } else {
            (None, first)
        };

        let ty = crate::syntax::Type::parse(ty_pair)?;
        let (mutable, name_pair) = if let Some(next) = inner.next() {
            if next.as_rule() == crate::parser::Rule::MutKeyword {
                let name_pair = inner
                    .next()
                    .ok_or(crate::parsing::error::ParseError::missing(
                        crate::parser::Rule::Identifier,
                    ))?;
                (true, name_pair)
            } else {
                (false, next)
            }
        } else {
            return Err(crate::parsing::error::ParseError::missing(
                crate::parser::Rule::Identifier,
            ));
        };
        let name = crate::syntax::Identifier::parse(name_pair)?;

        Ok(crate::syntax::Spanned::new(
            Self {
                modifier,
                mutable,
                name,
                ty,
            },
            span,
        ))
    }
}
