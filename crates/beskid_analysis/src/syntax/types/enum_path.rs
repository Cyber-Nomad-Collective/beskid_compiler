use crate::syntax::{Identifier, Path, Spanned};

use beskid_ast_derive::AstNode;

/// Qualified path naming an enum variant (`Module.Type::Variant`).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnumPath {
    #[ast(child)]
    pub type_path: Spanned<Path>,
    #[ast(child)]
    pub variant: Spanned<Identifier>,
}

impl crate::parsing::parsable::Parsable for EnumPath {
    fn parse(
        pair: pest::iterators::Pair<crate::parser::Rule>,
    ) -> Result<crate::syntax::Spanned<Self>, crate::parsing::error::ParseError> {
        if pair.as_rule() != crate::parser::Rule::EnumPath {
            return Err(crate::parsing::error::ParseError::unexpected_rule(pair, Some(crate::parser::Rule::EnumPath)));
        }

        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let type_path = crate::syntax::Path::parse(
            inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::Path))?,
        )?;
        let variant = crate::syntax::Identifier::parse(
            inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier))?,
        )?;

        Ok(crate::syntax::Spanned::new(Self { type_path, variant }, span))
    }
}
