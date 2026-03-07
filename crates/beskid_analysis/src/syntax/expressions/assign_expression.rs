use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Expression, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct AssignExpression {
    #[ast(child)]
    pub target: Box<Spanned<Expression>>,
    #[ast(skip)]
    pub op: Spanned<AssignOp>,
    #[ast(child)]
    pub value: Box<Spanned<Expression>>,
}

pub(crate) fn parse_assignment_expression(
    pair: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.into_inner();
    let target = Expression::parse(
        inner
            .next()
            .ok_or(ParseError::missing(Rule::LogicalOrExpression))?,
    )?;

    if let Some(operator_pair) = inner.next() {
        let operator = parse_assign_op(operator_pair)?;
        let value_pair = inner
            .next()
            .ok_or(ParseError::missing(Rule::AssignmentExpression))?;
        let value = Expression::parse(value_pair)?;
        let assign = Spanned::new(
            AssignExpression {
                target: Box::new(target),
                op: operator,
                value: Box::new(value),
            },
            span,
        );
        Ok(Spanned::new(Expression::Assign(assign), span))
    } else {
        Ok(Spanned::new(target.node, span))
    }
}

fn parse_assign_op(pair: Pair<Rule>) -> Result<Spanned<AssignOp>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let node = match pair.as_rule() {
        Rule::AssignmentOperator => match pair.as_str() {
            "=" => AssignOp::Assign,
            "+=" => AssignOp::AddAssign,
            "-=" => AssignOp::SubAssign,
            _ => return Err(ParseError::unexpected_rule(pair, Some(Rule::AssignmentOperator))),
        },
        _ => return Err(ParseError::unexpected_rule(pair, Some(Rule::AssignmentOperator))),
    };
    Ok(Spanned::new(node, span))
}
