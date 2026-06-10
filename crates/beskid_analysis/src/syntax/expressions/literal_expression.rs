use pest::Parser;
use pest::iterators::Pair;

use crate::parser::BeskidParser;
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::expressions::span::{remap_span, span_from_bounds};
use crate::syntax::expressions::string_decode::{split_string_literal_parts, StringLiteralPart};
use crate::syntax::{Expression, Literal, SpanInfo, Spanned};

use beskid_ast_derive::AstNode;

/// Expression consisting of a single [`Literal`]; string literals may desugar to concatenation.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiteralExpression {
    #[ast(child)]
    pub literal: Spanned<Literal>,
}

pub(crate) fn parse_literal_expression(
    pair: Pair<Rule>,
) -> Result<Spanned<Expression>, ParseError> {
    let input = pair.as_span().get_input();
    let span = SpanInfo::from_span(&pair.as_span());
    let literal = Literal::parse(pair)?;

    if let Literal::String(value) = &literal.node
        && let Some(expr) = try_desugar_interpolated_string(value, input, span)
    {
        return Ok(expr);
    }

    let literal_expr = Spanned::new(LiteralExpression { literal }, span);

    Ok(Spanned::new(Expression::Literal(literal_expr), span))
}

fn try_desugar_interpolated_string(
    source: &str,
    input: &str,
    literal_span: SpanInfo,
) -> Option<Spanned<Expression>> {
    let parts = split_string_literal_parts(source, input, literal_span).ok()?;
    if !parts
        .iter()
        .any(|part| matches!(part, StringLiteralPart::RuntimeInterpolation { .. }))
    {
        return None;
    }

    let mut interpolation_parts = Vec::new();
    for part in parts {
        match part {
            StringLiteralPart::Text { value, span } => {
                interpolation_parts.push(InterpolationPart::Text { text: value, span });
            }
            StringLiteralPart::RuntimeInterpolation {
                expression_source: _,
                span,
            } => {
                let expression = parse_interpolation_expression(input, span)?;
                interpolation_parts.push(InterpolationPart::Expr(expression));
            }
        }
    }

    build_interpolated_expression(interpolation_parts)
}

fn build_interpolated_expression(parts: Vec<InterpolationPart>) -> Option<Spanned<Expression>> {
    let parts = match parts.len() {
        1 => match parts.into_iter().next()? {
            InterpolationPart::Expr(expression) => {
                let span = expression.span;
                vec![
                    InterpolationPart::Text {
                        text: String::new(),
                        span,
                    },
                    InterpolationPart::Expr(expression),
                ]
            }
            other => vec![other],
        },
        _ => parts,
    };

    let mut iter = parts.into_iter().map(part_to_expression);
    let mut acc = iter.next()?;
    for next in iter {
        let combined_span = SpanInfo {
            start: acc.span.start,
            end: next.span.end,
            line_col_start: acc.span.line_col_start,
            line_col_end: next.span.line_col_end,
        };
        let op = Spanned::new(crate::syntax::BinaryOp::Add, acc.span);
        let binary = Spanned::new(
            crate::syntax::BinaryExpression {
                left: Box::new(acc),
                op,
                right: Box::new(next),
            },
            combined_span,
        );
        acc = Spanned::new(Expression::Binary(binary), combined_span);
    }
    Some(acc)
}

fn part_to_expression(part: InterpolationPart) -> Spanned<Expression> {
    match part {
        InterpolationPart::Text { text, span } => {
            let literal = Spanned::new(Literal::String(format!("\"{text}\"")), span);
            let literal_expr = Spanned::new(LiteralExpression { literal }, span);
            Spanned::new(Expression::Literal(literal_expr), span)
        }
        InterpolationPart::Expr(expression) => {
            let span = expression.span;
            Spanned::new(expression.node, span)
        }
    }
}

fn parse_interpolation_expression(
    source: &str,
    expr_span: SpanInfo,
) -> Option<Spanned<Expression>> {
    let expr_source = source.get(expr_span.start..expr_span.end)?;
    let mut pairs = BeskidParser::parse(Rule::Expression, expr_source).ok()?;
    let pair = pairs.next()?;
    if pairs.next().is_some() {
        return None;
    }

    let mut expression = Expression::parse(pair).ok()?;
    remap_expression_spans(&mut expression, expr_span.start, source);
    Some(expression)
}

fn remap_expression_spans(expression: &mut Spanned<Expression>, offset: usize, source: &str) {
    expression.span = remap_span(expression.span, offset, source);
    match &mut expression.node {
        Expression::Match(match_expr) => {
            match_expr.span = remap_span(match_expr.span, offset, source);
            remap_expression_spans(&mut match_expr.node.scrutinee, offset, source);
            for arm in &mut match_expr.node.arms {
                arm.span = remap_span(arm.span, offset, source);
                remap_pattern_spans(&mut arm.node.pattern, offset, source);
                if let Some(guard) = &mut arm.node.guard {
                    remap_expression_spans(guard, offset, source);
                }
                remap_expression_spans(&mut arm.node.value, offset, source);
            }
        }
        Expression::Lambda(lambda) => {
            lambda.span = remap_span(lambda.span, offset, source);
            for param in &mut lambda.node.parameters {
                param.span = remap_span(param.span, offset, source);
                param.node.name.span = remap_span(param.node.name.span, offset, source);
            }
            remap_expression_spans(&mut lambda.node.body, offset, source);
        }
        Expression::Assign(assign) => {
            assign.span = remap_span(assign.span, offset, source);
            remap_expression_spans(&mut assign.node.target, offset, source);
            assign.node.op.span = remap_span(assign.node.op.span, offset, source);
            remap_expression_spans(&mut assign.node.value, offset, source);
        }
        Expression::Binary(binary) => {
            binary.span = remap_span(binary.span, offset, source);
            remap_expression_spans(&mut binary.node.left, offset, source);
            binary.node.op.span = remap_span(binary.node.op.span, offset, source);
            remap_expression_spans(&mut binary.node.right, offset, source);
        }
        Expression::Unary(unary) => {
            unary.span = remap_span(unary.span, offset, source);
            unary.node.op.span = remap_span(unary.node.op.span, offset, source);
            remap_expression_spans(&mut unary.node.expr, offset, source);
        }
        Expression::Call(call) => {
            call.span = remap_span(call.span, offset, source);
            remap_expression_spans(&mut call.node.callee, offset, source);
            for arg in &mut call.node.args {
                remap_expression_spans(arg, offset, source);
            }
        }
        Expression::Member(member) => {
            member.span = remap_span(member.span, offset, source);
            remap_expression_spans(&mut member.node.target, offset, source);
            member.node.member.span = remap_span(member.node.member.span, offset, source);
        }
        Expression::Literal(literal) => {
            literal.span = remap_span(literal.span, offset, source);
            literal.node.literal.span = remap_span(literal.node.literal.span, offset, source);
        }
        Expression::Path(path) => {
            path.span = remap_span(path.span, offset, source);
            remap_path_spans(&mut path.node.path, offset, source);
        }
        Expression::StructLiteral(literal) => {
            literal.span = remap_span(literal.span, offset, source);
            remap_path_spans(&mut literal.node.path, offset, source);
            for field in &mut literal.node.fields {
                field.span = remap_span(field.span, offset, source);
                field.node.name.span = remap_span(field.node.name.span, offset, source);
                remap_expression_spans(&mut field.node.value, offset, source);
            }
        }
        Expression::EnumConstructor(constructor) => {
            constructor.span = remap_span(constructor.span, offset, source);
            constructor.node.path.span = remap_span(constructor.node.path.span, offset, source);
            remap_path_spans(&mut constructor.node.path.node.type_path, offset, source);
            constructor.node.path.node.variant.span =
                remap_span(constructor.node.path.node.variant.span, offset, source);
            for arg in &mut constructor.node.args {
                remap_expression_spans(arg, offset, source);
            }
        }
        Expression::Block(block) => {
            block.span = remap_span(block.span, offset, source);
            remap_block_spans(&mut block.node.block, offset, source);
        }
        Expression::Grouped(grouped) => {
            grouped.span = remap_span(grouped.span, offset, source);
            remap_expression_spans(&mut grouped.node.expr, offset, source);
        }
        Expression::Try(try_expr) => {
            try_expr.span = remap_span(try_expr.span, offset, source);
            remap_expression_spans(&mut try_expr.node.expr, offset, source);
        }
        Expression::Spawn(spawn) => {
            spawn.span = remap_span(spawn.span, offset, source);
            remap_expression_spans(spawn.node.callee.as_mut(), offset, source);
        }
        Expression::Index(index) => {
            index.span = remap_span(index.span, offset, source);
            remap_expression_spans(index.node.target.as_mut(), offset, source);
            remap_expression_spans(index.node.index.as_mut(), offset, source);
        }
        Expression::ArrayLiteral(literal) => {
            literal.span = remap_span(literal.span, offset, source);
            for element in &mut literal.node.elements {
                remap_expression_spans(element, offset, source);
            }
        }
        Expression::MacroInvocation(_) | Expression::MacroMetavariable(_) | Expression::CodeString(_) => {}
    }
}

fn remap_path_spans(path: &mut Spanned<crate::syntax::Path>, offset: usize, source: &str) {
    path.span = remap_span(path.span, offset, source);
    for segment in &mut path.node.segments {
        segment.span = remap_span(segment.span, offset, source);
        segment.node.name.span = remap_span(segment.node.name.span, offset, source);
        for type_arg in &mut segment.node.type_args {
            remap_type_spans(type_arg, offset, source);
        }
    }
}

fn remap_type_spans(ty: &mut Spanned<crate::syntax::Type>, offset: usize, source: &str) {
    ty.span = remap_span(ty.span, offset, source);
    match &mut ty.node {
        crate::syntax::Type::Complex(path) => remap_path_spans(path, offset, source),
        crate::syntax::Type::Array(inner) => {
            remap_type_spans(inner, offset, source);
        }
        crate::syntax::Type::Function {
            return_type,
            parameters,
        } => {
            remap_type_spans(return_type, offset, source);
            for parameter in parameters {
                remap_type_spans(parameter, offset, source);
            }
        }
        crate::syntax::Type::Primitive(_) => {}
    }
}

fn remap_pattern_spans(pattern: &mut Spanned<crate::syntax::Pattern>, offset: usize, source: &str) {
    pattern.span = remap_span(pattern.span, offset, source);
    match &mut pattern.node {
        crate::syntax::Pattern::Wildcard => {}
        crate::syntax::Pattern::Identifier(name) => {
            name.span = remap_span(name.span, offset, source);
        }
        crate::syntax::Pattern::Literal(literal) => {
            literal.span = remap_span(literal.span, offset, source);
        }
        crate::syntax::Pattern::Enum(enum_pattern) => {
            enum_pattern.span = remap_span(enum_pattern.span, offset, source);
            enum_pattern.node.path.span = remap_span(enum_pattern.node.path.span, offset, source);
            remap_path_spans(&mut enum_pattern.node.path.node.type_path, offset, source);
            enum_pattern.node.path.node.variant.span =
                remap_span(enum_pattern.node.path.node.variant.span, offset, source);
            for item in &mut enum_pattern.node.items {
                remap_pattern_spans(item, offset, source);
            }
        }
    }
}

fn remap_else_branch_spans(
    else_branch: &mut Spanned<crate::syntax::ElseBranch>,
    offset: usize,
    source: &str,
) {
    else_branch.span = remap_span(else_branch.span, offset, source);
    match &mut else_branch.node {
        crate::syntax::ElseBranch::Block(block) => remap_block_spans(block, offset, source),
        crate::syntax::ElseBranch::If(nested) => {
            nested.span = remap_span(nested.span, offset, source);
            remap_expression_spans(&mut nested.node.condition, offset, source);
            remap_block_spans(&mut nested.node.then_block, offset, source);
            if let Some(nested_else) = &mut nested.node.else_branch {
                remap_else_branch_spans(nested_else, offset, source);
            }
        }
    }
}

fn remap_block_spans(block: &mut Spanned<crate::syntax::Block>, offset: usize, source: &str) {
    block.span = remap_span(block.span, offset, source);
    for statement in &mut block.node.statements {
        remap_statement_spans(statement, offset, source);
    }
}

fn remap_statement_spans(
    statement: &mut Spanned<crate::syntax::Statement>,
    offset: usize,
    source: &str,
) {
    statement.span = remap_span(statement.span, offset, source);
    use crate::syntax::Statement;
    match &mut statement.node {
        Statement::Let(let_stmt) => {
            let_stmt.span = remap_span(let_stmt.span, offset, source);
            let_stmt.node.name.span = remap_span(let_stmt.node.name.span, offset, source);
            if let Some(ty) = &mut let_stmt.node.type_annotation {
                remap_type_spans(ty, offset, source);
            }
            remap_expression_spans(&mut let_stmt.node.value, offset, source);
        }
        Statement::Return(return_stmt) => {
            return_stmt.span = remap_span(return_stmt.span, offset, source);
            if let Some(value) = &mut return_stmt.node.value {
                remap_expression_spans(value, offset, source);
            }
        }
        Statement::Expression(expr_stmt) => {
            expr_stmt.span = remap_span(expr_stmt.span, offset, source);
            remap_expression_spans(&mut expr_stmt.node.expression, offset, source);
        }
        Statement::If(if_stmt) => {
            if_stmt.span = remap_span(if_stmt.span, offset, source);
            remap_expression_spans(&mut if_stmt.node.condition, offset, source);
            remap_block_spans(&mut if_stmt.node.then_block, offset, source);
            if let Some(else_branch) = &mut if_stmt.node.else_branch {
                remap_else_branch_spans(else_branch, offset, source);
            }
        }
        Statement::While(while_stmt) => {
            while_stmt.span = remap_span(while_stmt.span, offset, source);
            remap_expression_spans(&mut while_stmt.node.condition, offset, source);
            remap_block_spans(&mut while_stmt.node.body, offset, source);
        }
        Statement::For(for_stmt) => {
            for_stmt.span = remap_span(for_stmt.span, offset, source);
            for_stmt.node.iterator.span = remap_span(for_stmt.node.iterator.span, offset, source);
            remap_expression_spans(&mut for_stmt.node.iterable, offset, source);
            remap_block_spans(&mut for_stmt.node.body, offset, source);
        }
        Statement::Break(break_stmt) => {
            break_stmt.span = remap_span(break_stmt.span, offset, source);
        }
        Statement::Continue(continue_stmt) => {
            continue_stmt.span = remap_span(continue_stmt.span, offset, source);
        }
        Statement::With(_) | Statement::Launch(_) => {}
    }
}

enum InterpolationPart {
    Text { text: String, span: SpanInfo },
    Expr(Spanned<Expression>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::BeskidParser;
    use crate::parsing::parsable::Parsable;
    use pest::Parser;

    #[test]
    fn desugars_string_interpolation_to_binary_add() {
        let input = "\"hi ${name}\"";
        let mut pairs = BeskidParser::parse(Rule::Expression, input).expect("parse expression");
        let pair = pairs.next().expect("expression pair");
        let expr = Expression::parse(pair).expect("expression ast");
        assert!(
            matches!(expr.node, Expression::Binary(_)),
            "expected desugared binary expression, got {:?}",
            expr.node
        );
    }

    #[test]
    fn desugars_single_interpolation_expr_to_empty_string_concat() {
        let input = "\"${code}\"";
        let mut pairs = BeskidParser::parse(Rule::Expression, input).expect("parse expression");
        let pair = pairs.next().expect("expression pair");
        let expr = Expression::parse(pair).expect("expression ast");
        assert!(
            matches!(expr.node, Expression::Binary(_)),
            "expected lone interpolation to desugar to empty-string concat"
        );
    }
}
