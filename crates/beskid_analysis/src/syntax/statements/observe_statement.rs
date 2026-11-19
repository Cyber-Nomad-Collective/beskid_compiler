use beskid_ast_derive::AstNode;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, MemberExpression, SpanInfo, Spanned};

/// Registers a callback on the receiver's stored property.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ObserveStatement {
    #[ast(child)]
    pub target: Spanned<Expression>,
    #[ast(child)]
    pub callback: Spanned<Expression>,
}

impl Parsable for ObserveStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let target_pair = inner.next().ok_or(ParseError::missing(Rule::ObserveTarget))?;
        let target = parse_target(target_pair)?;
        let callback = Expression::parse(inner.next().ok_or(ParseError::missing(Rule::LambdaExpression))?)?;
        Ok(Spanned::new(Self { target, callback }, span))
    }
}

fn parse_target(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let expression = Expression::parse(pair.clone())?;
    let span = expression.span;
    match expression.node {
        Expression::Member(_) => Ok(expression),
        // Ordinary dotted names parse as paths. An observation always projects their last
        // component, while the receiver prefix keeps its normal name-resolution rules.
        Expression::Path(mut path) if path.node.path.node.segments.len() > 1 => {
            let input = pair.as_span().get_input();
            let prefix_end = pair
                .into_inner()
                .next()
                .and_then(|primary| primary.into_inner().next())
                .and_then(|path| path.into_inner().rev().nth(1))
                .ok_or(ParseError::missing(Rule::PathSegment))?
                .as_span()
                .end();
            let prefix_span = crate::syntax::expressions::span::trim_trailing_trivia(
                input,
                SpanInfo::from_byte_range_in_source(input, span.start, prefix_end),
            );
            let member = path.node.path.node.segments.pop().ok_or(ParseError::missing(Rule::MemberAccess))?.node;
            path.span = prefix_span;
            path.node.path.span = prefix_span;
            let node = MemberExpression {
                target: Box::new(Spanned::new(Expression::Path(path), prefix_span)),
                member: member.name,
                type_args: member.type_args,
            };
            Ok(Spanned::new(Expression::Member(Spanned::new(node, span)), span))
        }
        _ => Err(ParseError::unexpected_rule(pair, Some(Rule::MemberAccess))),
    }
}
