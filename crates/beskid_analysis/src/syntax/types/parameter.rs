use crate::syntax::{Identifier, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Function or method parameter: optional `mut`, type, and name.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Parameter {
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
            return Err(crate::parsing::error::ParseError::unexpected_rule(pair, Some(crate::parser::Rule::Parameter)));
        }

        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let first = inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?;

        let (mutable, ty_pair) = if first.as_rule() == crate::parser::Rule::MutKeyword {
            let ty_pair =
                inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?;
            (true, ty_pair)
        } else {
            (false, first)
        };

        let ty = crate::syntax::Type::parse(ty_pair)?;
        let name_pair =
            inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier))?;
        let name = crate::syntax::Identifier::parse(name_pair)?;

        Ok(crate::syntax::Spanned::new(Self { mutable, name, ty }, span))
    }
}
