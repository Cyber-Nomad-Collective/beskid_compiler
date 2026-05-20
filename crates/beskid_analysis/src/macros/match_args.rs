//! Macro invocation argument matching against formal parameters.

use crate::syntax::expressions::Expression;
use crate::syntax::items::{MacroFragmentKind, MacroParameter};
use crate::syntax::statements::{Block, Statement};
use crate::syntax::types::Type;
use crate::syntax::{EnumPattern, Identifier, Pattern, Path, SpanInfo, Spanned};

use super::registry::macro_name_key;

/// Captured macro argument for substitution (one variant per fragment kind).
#[derive(Debug, Clone)]
pub enum FragmentBinding {
    Block(Spanned<Block>),
    Expression(Spanned<Expression>),
    Statement(Spanned<Statement>),
    Type(Spanned<Type>),
    Identifier(Spanned<Identifier>),
    Literal(Spanned<crate::syntax::Literal>),
    Pattern(Spanned<Pattern>),
    Path(Spanned<Path>),
    Item(Spanned<crate::syntax::items::Node>),
    Node(Spanned<Expression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchError {
    UnknownMacro { name: String, span: SpanInfo },
    ArityMismatch {
        name: String,
        expected: usize,
        actual: usize,
        span: SpanInfo,
    },
    KindMismatch {
        name: String,
        parameter: String,
        expected_kind: String,
        span: SpanInfo,
    },
}

pub fn fragment_kind_keyword(kind: MacroFragmentKind) -> &'static str {
    match kind {
        MacroFragmentKind::Block => "block",
        MacroFragmentKind::Expression => "expression",
        MacroFragmentKind::Statement => "statement",
        MacroFragmentKind::Type => "type",
        MacroFragmentKind::Identifier => "identifier",
        MacroFragmentKind::Literal => "literal",
        MacroFragmentKind::Pattern => "pattern",
        MacroFragmentKind::Path => "path",
        MacroFragmentKind::Item => "item",
        MacroFragmentKind::Node => "node",
    }
}

pub fn match_arguments(
    macro_name: &Spanned<Identifier>,
    parameters: &[Spanned<MacroParameter>],
    args: &[Spanned<Expression>],
    block: Option<&Spanned<Block>>,
) -> Result<Vec<(String, FragmentBinding)>, MatchError> {
    let name = macro_name_key(macro_name);
    let invocation_span = macro_name.span;

    let expected_expr_args = parameters
        .iter()
        .filter(|p| p.node.kind.node != MacroFragmentKind::Block)
        .count();
    if expected_expr_args != args.len() {
        return Err(MatchError::ArityMismatch {
            name: name.clone(),
            expected: expected_expr_args,
            actual: args.len(),
            span: invocation_span,
        });
    }

    if parameters
        .iter()
        .any(|p| p.node.kind.node == MacroFragmentKind::Block)
        && block.is_none()
    {
        return Err(MatchError::ArityMismatch {
            name: name.clone(),
            expected: expected_expr_args,
            actual: args.len(),
            span: invocation_span,
        });
    }

    let mut bindings = Vec::new();
    let mut arg_index = 0usize;
    for param in parameters {
        let param_name = param.node.name.node.name.clone();
        let kind = param.node.kind.node;
        let binding = match kind {
            MacroFragmentKind::Block => FragmentBinding::Block(
                block
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?
                    .clone(),
            ),
            MacroFragmentKind::Expression => {
                FragmentBinding::Expression(next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
            }
            MacroFragmentKind::Statement => FragmentBinding::Statement(
                expression_as_statement(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Type => FragmentBinding::Type(
                expression_as_type(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Identifier => FragmentBinding::Identifier(
                expression_as_identifier(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Literal => FragmentBinding::Literal(
                expression_as_literal(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Pattern => FragmentBinding::Pattern(
                expression_as_pattern(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Path => FragmentBinding::Path(
                expression_as_path(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Item => FragmentBinding::Item(
                expression_as_item(&next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
                    .ok_or_else(|| kind_error(&name, &param_name, kind, param.span))?,
            ),
            MacroFragmentKind::Node => {
                FragmentBinding::Node(next_arg(args, &mut arg_index, &name, expected_expr_args, invocation_span)?)
            }
        };
        bindings.push((param_name, binding));
    }

    Ok(bindings)
}

fn kind_error(
    name: &str,
    parameter: &str,
    kind: MacroFragmentKind,
    span: SpanInfo,
) -> MatchError {
    MatchError::KindMismatch {
        name: name.to_string(),
        parameter: parameter.to_string(),
        expected_kind: fragment_kind_keyword(kind).to_string(),
        span,
    }
}

fn next_arg(
    args: &[Spanned<Expression>],
    arg_index: &mut usize,
    name: &str,
    expected: usize,
    span: SpanInfo,
) -> Result<Spanned<Expression>, MatchError> {
    let expr = args.get(*arg_index).ok_or_else(|| MatchError::ArityMismatch {
        name: name.to_string(),
        expected,
        actual: args.len(),
        span,
    })?;
    *arg_index += 1;
    Ok(expr.clone())
}

fn expression_as_statement(expr: &Spanned<Expression>) -> Option<Spanned<Statement>> {
    match &expr.node {
        Expression::Block(b) if b.node.block.node.statements.len() == 1 => {
            Some(b.node.block.node.statements[0].clone())
        }
        _ => Some(Spanned::new(
            Statement::Expression(Spanned::new(
                crate::syntax::ExpressionStatement {
                    expression: expr.clone(),
                },
                expr.span,
            )),
            expr.span,
        )),
    }
}

fn expression_as_type(expr: &Spanned<Expression>) -> Option<Spanned<Type>> {
    match &expr.node {
        Expression::Path(p) => Some(Spanned::new(Type::Complex(p.node.path.clone()), expr.span)),
        _ => None,
    }
}

fn expression_as_identifier(expr: &Spanned<Expression>) -> Option<Spanned<Identifier>> {
    let path = expression_as_path(expr)?;
    if path.node.segments.len() == 1 {
        Some(path.node.segments[0].node.name.clone())
    } else {
        None
    }
}

fn expression_as_literal(expr: &Spanned<Expression>) -> Option<Spanned<crate::syntax::Literal>> {
    match &expr.node {
        Expression::Literal(l) => Some(l.node.literal.clone()),
        _ => None,
    }
}

fn expression_as_pattern(expr: &Spanned<Expression>) -> Option<Spanned<Pattern>> {
    match &expr.node {
        Expression::Literal(l) => Some(Spanned::new(Pattern::Literal(l.node.literal.clone()), expr.span)),
        Expression::Path(p) if p.node.path.node.segments.len() == 1 => Some(Spanned::new(
            Pattern::Identifier(p.node.path.node.segments[0].node.name.clone()),
            expr.span,
        )),
        Expression::EnumConstructor(e) => {
            let items: Vec<_> = e
                .node
                .args
                .iter()
                .filter_map(expression_as_pattern)
                .collect();
            if items.len() != e.node.args.len() {
                return None;
            }
            Some(Spanned::new(
                Pattern::Enum(Spanned::new(
                    EnumPattern {
                        path: e.node.path.clone(),
                        items,
                    },
                    expr.span,
                )),
                expr.span,
            ))
        }
        _ => None,
    }
}

fn expression_as_path(expr: &Spanned<Expression>) -> Option<Spanned<Path>> {
    match &expr.node {
        Expression::Path(p) => Some(p.node.path.clone()),
        _ => None,
    }
}

/// v1: `item` fragment actuals accept path expressions as `use` surrogates only.
pub fn expression_as_item(expr: &Spanned<Expression>) -> Option<Spanned<crate::syntax::items::Node>> {
    use crate::syntax::items::Node;

    match &expr.node {
        Expression::Path(p) => Some(Spanned::new(
            Node::UseDeclaration(Spanned::new(
                crate::syntax::UseDeclaration {
                    visibility: Spanned::new(crate::syntax::Visibility::Private, expr.span),
                    path: p.node.path.clone(),
                    alias: None,
                },
                expr.span,
            )),
            expr.span,
        )),
        _ => None,
    }
}
