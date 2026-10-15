use crate::syntax::{Attribute, Identifier, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Function or method parameter: optional `bulk` calling-convention modifier,
/// optional `mut` or `move`, type, and name. `bulk` is the outermost modifier (a bulk parameter
/// may also be mutable or owning); the flags are parser-derived and retained in the SDK
/// constructor schema. `move` declares an owning parameter: the caller's value is consumed.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Parameter {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(skip)]
    pub bulk: bool,
    #[ast(skip)]
    pub mutable: bool,
    #[ast(skip)]
    pub movable: bool,
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
        let mut inner = pair.into_inner().peekable();
        let attributes = crate::syntax::items::parse_helpers::parse_attributes(&mut inner)?;
        let first = inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?;

        // Leading modifiers appear in fixed order: `bulk` (outermost calling-convention
        // modifier) then one of `mut` or `move`. Consume the modifier pairs before the type.
        let mut bulk = false;
        let mut mutable = false;
        let mut movable = false;
        let mut current = first;
        if current.as_rule() == crate::parser::Rule::BulkKeyword {
            bulk = true;
            current = inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?;
        }
        match current.as_rule() {
            crate::parser::Rule::MutKeyword => mutable = true,
            crate::parser::Rule::MoveKeyword => movable = true,
            _ => {}
        }
        let ty_pair = if mutable || movable {
            inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?
        } else {
            current
        };

        let ty = crate::syntax::Type::parse(ty_pair)?;
        let name_pair =
            inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier))?;
        let name = crate::syntax::Identifier::parse(name_pair)?;

        Ok(crate::syntax::Spanned::new(Self { attributes, bulk, mutable, movable, name, ty }, span))
    }
}
