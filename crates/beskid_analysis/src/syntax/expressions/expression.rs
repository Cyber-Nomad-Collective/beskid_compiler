use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::expressions::span::span_from_bounds;
use crate::syntax::{SpanInfo, Spanned};
use pest::iterators::Pair;

use crate::syntax::Identifier;

use super::array_literal_expression::parse_array_literal_expression;
use super::assign_expression::AssignExpression;
use super::binary_expression::{BinaryExpression, parse_binary_expression};
use super::block_expression::parse_block_expression;
use super::call_expression::parse_call_expression;
use super::code_string::CodeStringLiteral;
use super::enum_constructor_expression::parse_enum_constructor_expression;
use super::grouped_expression::parse_grouped_expression;
use super::index_expression::parse_index_expression;
use super::lambda_expression::parse_lambda_expression;
use super::literal_expression::parse_literal_expression;
use super::macro_invocation::MacroInvocation;
use super::macro_metavariable::MacroMetavariable;
use super::match_expression::parse_match_expression;
use super::member_expression::parse_member_expression;
use super::path_expression::parse_path_expression;
use super::spawn_expression::parse_spawn_unary;
use super::struct_literal_expression::parse_struct_literal_expression;
use super::try_expression::TryExpression;
use super::unary_expression::{UnaryExpression, parse_prefix_unary_expression};

use beskid_ast_derive::AstNode;

/// Top-level expression shape after parsing (postfix chains, operators, literals, etc.).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Expression {
    #[ast(child)]
    Match(Spanned<super::match_expression::MatchExpression>),
    #[ast(child)]
    Lambda(Spanned<super::lambda_expression::LambdaExpression>),
    #[ast(child)]
    Assign(Spanned<AssignExpression>),
    #[ast(child)]
    Binary(Spanned<BinaryExpression>),
    #[ast(child)]
    Unary(Spanned<UnaryExpression>),
    #[ast(child)]
    Call(Spanned<super::call_expression::CallExpression>),
    #[ast(child)]
    Member(Spanned<super::member_expression::MemberExpression>),
    #[ast(child)]
    Literal(Spanned<super::literal_expression::LiteralExpression>),
    #[ast(child)]
    Path(Spanned<super::path_expression::PathExpression>),
    #[ast(child)]
    StructLiteral(Spanned<super::struct_literal_expression::StructLiteralExpression>),
    #[ast(child)]
    EnumConstructor(Spanned<super::enum_constructor_expression::EnumConstructorExpression>),
    #[ast(child)]
    Block(Spanned<super::block_expression::BlockExpression>),
    #[ast(child)]
    Grouped(Spanned<super::grouped_expression::GroupedExpression>),
    #[ast(child)]
    Try(Spanned<TryExpression>),
    #[ast(child)]
    Spawn(Spanned<super::spawn_expression::SpawnExpression>),
    #[ast(child)]
    MacroInvocation(Spanned<MacroInvocation>),
    #[ast(child)]
    MacroMetavariable(Spanned<MacroMetavariable>),
    #[ast(child)]
    Index(Spanned<super::index_expression::IndexExpression>),
    #[ast(child)]
    ArrayLiteral(Spanned<super::array_literal_expression::ArrayLiteralExpression>),
    #[ast(child)]
    CodeString(Spanned<super::code_string::CodeStringLiteral>),
}

impl Parsable for Expression {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        parse_expression(pair)
    }
}

pub(crate) fn parse_expression(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());

    match pair.as_rule() {
        Rule::Expression => {
            let inner = pair.into_inner().next().ok_or(ParseError::missing(Rule::AssignmentExpression))?;
            let inner_expr = parse_expression(inner)?;
            Ok(Spanned::new(inner_expr.node, span))
        }
        Rule::LambdaExpression => parse_lambda_expression(pair),
        Rule::MatchExpression => parse_match_expression(pair),
        Rule::AssignmentExpression => super::assign_expression::parse_assignment_expression(pair),
        Rule::LogicalOrExpression
        | Rule::LogicalAndExpression
        | Rule::BitwiseOrExpression
        | Rule::BitwiseAndExpression
        | Rule::EqualityExpression
        | Rule::ComparisonExpression
        | Rule::ShiftExpression
        | Rule::AdditionExpression
        | Rule::MultiplicationExpression => parse_binary_expression(pair),
        Rule::UnaryExpression => {
            let inner = pair.into_inner().next().ok_or(ParseError::missing(Rule::UnaryExpression))?;
            match inner.as_rule() {
                Rule::SpawnUnary => parse_spawn_unary(inner),
                Rule::PrefixUnary => parse_prefix_unary_expression(inner),
                _ => Err(ParseError::unexpected_rule(inner, Some(Rule::UnaryExpression))),
            }
        }
        Rule::SpawnUnary => parse_spawn_unary(pair),
        Rule::PrefixUnary => parse_prefix_unary_expression(pair),
        Rule::PostfixExpression => parse_postfix_expression(pair),
        Rule::PrimaryExpression => parse_primary_expression(pair),
        Rule::GroupedExpression => parse_grouped_expression(pair),
        Rule::BlockExpression => parse_block_expression(pair),
        Rule::EnumConstructorExpression => parse_enum_constructor_expression(pair),
        Rule::StructLiteralExpression => parse_struct_literal_expression(pair),
        Rule::ArrayLiteralExpression => parse_array_literal_expression(pair),
        Rule::CodeExpression => {
            let node = CodeStringLiteral::parse(pair)?;
            Ok(Spanned::new(Expression::CodeString(node), span))
        }
        Rule::Literal => parse_literal_expression(pair),
        Rule::MacroInvocation => {
            let node = MacroInvocation::parse(pair)?;
            Ok(Spanned::new(Expression::MacroInvocation(node), span))
        }
        Rule::MacroMetavariable => {
            let node = MacroMetavariable::parse(pair)?;
            Ok(Spanned::new(Expression::MacroMetavariable(node), span))
        }
        Rule::Path => parse_path_expression(pair),
        Rule::TryBlockExpression => {
            let mut inner = pair.into_inner();
            let body_pair = inner.next().ok_or(ParseError::missing(Rule::Block))?;
            let body_expr = parse_block_expression(body_pair)?;
            let error_var_pair = inner.next().ok_or(ParseError::missing(Rule::Identifier))?;
            let error_var = Identifier::parse(error_var_pair)?;
            let catch_block_pair = inner.next().ok_or(ParseError::missing(Rule::Block))?;
            let catch_span = SpanInfo::from_span(&catch_block_pair.as_span());
            let catch_expr = parse_block_expression(catch_block_pair)?;
            let catch_block = match catch_expr.node {
                Expression::Block(block) => block,
                _ => {
                    return Err(ParseError::UnexpectedRule {
                        expected: Some(Rule::Block),
                        found: Rule::Block,
                        span: catch_span,
                    });
                }
            };
            let try_node = TryExpression {
                expr: Box::new(body_expr),
                error_variable: Some(error_var),
                catch_block: Some(catch_block),
            };
            Ok(Spanned::new(Expression::Try(Spanned::new(try_node, span)), span))
        }
        _ => Err(ParseError::unexpected_rule(pair, None)),
    }
}

/// Parses a postfix chain: calls, member access, and `?` try wrapping.
pub(crate) fn parse_postfix_expression(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let input = pair.as_span().get_input();
    let mut inner = pair.into_inner();
    let primary = parse_primary_expression(inner.next().ok_or(ParseError::missing(Rule::PrimaryExpression))?)?;
    let mut expr = primary;

    for op_pair in inner {
        let end = op_pair.as_span().end();
        let operator = match op_pair.as_rule() {
            Rule::PostfixOperator => op_pair.into_inner().next().ok_or(ParseError::missing(Rule::PostfixOperator))?,
            _ => op_pair,
        };

        expr = match operator.as_rule() {
            Rule::CallOperator => parse_call_expression(expr, operator)?,
            Rule::MemberAccess => parse_member_expression(expr, operator)?,
            Rule::SubscriptOperator => parse_index_expression(expr, operator)?,
            Rule::TryOperator => {
                let expr_span = expr.span;
                let try_node = TryExpression { expr: Box::new(expr), error_variable: None, catch_block: None };
                Spanned::new(Expression::Try(Spanned::new(try_node, expr_span)), expr_span)
            }
            _ => return Err(ParseError::unexpected_rule(operator, None)),
        };

        let node_span =
            span_from_bounds(input, expr.span.start, end).ok_or(ParseError::missing(Rule::PostfixExpression))?;
        expr = Spanned::new(expr.node, node_span);
    }

    Ok(Spanned::new(expr.node, span))
}

/// Parses the innermost `PrimaryExpression` / delegate to nested expression rules.
pub(crate) fn parse_primary_expression(pair: Pair<Rule>) -> Result<Spanned<Expression>, ParseError> {
    let inner = pair.into_inner().next().ok_or(ParseError::missing(Rule::Expression))?;
    parse_expression(inner)
}
