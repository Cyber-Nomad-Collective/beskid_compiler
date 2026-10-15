use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, Identifier, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Field or member access (`expr.member`).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemberExpression {
    #[ast(child)]
    pub target: Box<Spanned<Expression>>,
    #[ast(child)]
    pub member: Spanned<Identifier>,
    /// Explicit method type arguments written after the member name (`.Map<i64>`).
    #[ast(children)]
    pub type_args: Vec<Spanned<Type>>,
}

pub(crate) fn parse_member_expression(
    target: Spanned<Expression>,
    pair: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();
    let member = inner.next().ok_or(ParseError::missing(Rule::Identifier))?;
    let member = Identifier::parse(member)?;
    let type_args = match inner.next() {
        Some(arguments) => arguments.into_inner().map(Type::parse).collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let member_expr = Spanned::new(MemberExpression { target: Box::new(target), member, type_args }, span);

    Ok(Spanned::new(Expression::Member(member_expr), span))
}
