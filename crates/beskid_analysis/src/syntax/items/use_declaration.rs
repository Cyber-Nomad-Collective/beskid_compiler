use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::parse_visibility_or_default;
use crate::syntax::{Identifier, Path, SpanInfo, Spanned, Visibility};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct UseDeclaration {
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub path: Spanned<Path>,
    #[ast(child)]
    pub alias: Option<Spanned<Identifier>>,
}

impl Parsable for UseDeclaration {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let path = Path::parse(inner.next().ok_or(ParseError::missing(Rule::Path))?)?;
        let alias = inner.next().map(Identifier::parse).transpose()?;

        Ok(Spanned::new(
            Self {
                visibility,
                path,
                alias,
            },
            span,
        ))
    }
}
