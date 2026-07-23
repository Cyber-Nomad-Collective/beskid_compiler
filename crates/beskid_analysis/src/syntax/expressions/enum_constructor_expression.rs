use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{EnumPath, Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// Enum variant construction `Type.Variant(args...)`.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnumConstructorExpression {
    #[ast(child)]
    pub path: Spanned<EnumPath>,
    pub has_empty_parens: bool,
    #[ast(children)]
    pub args: Vec<Spanned<Expression>>,
}

pub(crate) fn parse_enum_constructor_expression(
    pair: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let variant = pair
        .into_inner()
        .next()
        .ok_or(ParseError::missing(Rule::EnumConstructorExpression))?;
    let mut inner = variant.clone().into_inner();
    let has_empty_parens = variant.as_rule() == Rule::enum_constructor_with_args;
    let path = EnumPath::parse(inner.next().ok_or(ParseError::missing(Rule::EnumPath))?)?;
    let args = if let Some(arg_list) = inner.next() {
        arg_list
            .into_inner()
            .map(Expression::parse)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    let constructor = Spanned::new(
        EnumConstructorExpression {
            path,
            has_empty_parens,
            args,
        },
        span,
    );

    Ok(Spanned::new(Expression::EnumConstructor(constructor), span))
}
