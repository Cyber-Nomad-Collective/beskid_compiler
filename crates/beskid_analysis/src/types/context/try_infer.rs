//! Infer enum metadata for `?` desugaring before full program type-check.

use std::collections::HashMap;

use crate::hir::{HirExpressionNode, HirItem, HirProgram, HirStatementNode};
use crate::resolve::Resolution;
use crate::syntax::{SpanInfo, Spanned};

use super::context::TypeContext;

/// Resolved enum used when lowering `expr?` to `match`.
#[derive(Debug, Clone)]
pub struct TryDesugarTarget {
    pub type_name: String,
    pub ok_variant: String,
}

pub fn try_desugar_target_for_operand(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    operand: &Spanned<HirExpressionNode>,
) -> Option<TryDesugarTarget> {
    let mut ctx = TypeContext::new(resolution);
    ctx.seed_types();
    for program in programs {
        ctx.seed_enum_definitions(program);
    }
    let target_type = ctx.type_expression(operand)?;
    let item_id = ctx.named_item_id(target_type)?;
    let type_name = resolution
        .items
        .iter()
        .find(|info| info.id == item_id)
        .map(|info| info.name.clone())?;
    let ok_variant = ctx.ok_variant_name(item_id, "Ok")?;
    Some(TryDesugarTarget {
        type_name,
        ok_variant,
    })
}

/// Spans of `?` operands that are not a `Result`-shaped enum (semantic stage 7 / early IDE).
pub fn invalid_try_expression_spans(
    resolution: &Resolution,
    entry: &Spanned<HirProgram>,
) -> Vec<SpanInfo> {
    let programs: Vec<&Spanned<HirProgram>> = vec![entry];
    let mut spans = Vec::new();
    collect_invalid_try_targets(resolution, &programs, entry, &mut spans);
    spans
}

fn collect_invalid_try_targets(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    program: &Spanned<HirProgram>,
    spans: &mut Vec<SpanInfo>,
) {
    for item in &program.node.items {
        collect_invalid_try_targets_item(resolution, programs, item, spans);
    }
}

fn collect_invalid_try_targets_item(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    item: &Spanned<HirItem>,
    spans: &mut Vec<SpanInfo>,
) {
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            collect_invalid_try_targets_in_block(resolution, programs, &def.node.body, spans);
        }
        HirItem::MethodDefinition(def) => {
            collect_invalid_try_targets_in_block(resolution, programs, &def.node.body, spans);
        }
        HirItem::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_invalid_try_targets_item(resolution, programs, nested, spans);
            }
        }
        _ => {}
    }
}

fn collect_invalid_try_targets_in_block(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    block: &Spanned<crate::hir::HirBlock>,
    spans: &mut Vec<SpanInfo>,
) {
    for statement in &block.node.statements {
        if let HirStatementNode::ExpressionStatement(expr_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &expr_stmt.node.expression,
                spans,
            );
        } else if let HirStatementNode::LetStatement(let_stmt) = &statement.node {
            collect_invalid_try_targets_in_expression(resolution, programs, &let_stmt.node.value, spans);
        } else if let HirStatementNode::ReturnStatement(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value {
                collect_invalid_try_targets_in_expression(resolution, programs, value, spans);
            }
    }
}

fn collect_invalid_try_targets_in_expression(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    expr: &Spanned<HirExpressionNode>,
    spans: &mut Vec<SpanInfo>,
) {
    if let HirExpressionNode::TryExpression(try_expr) = &expr.node
        && try_desugar_target_for_operand(resolution, programs, &try_expr.node.expr).is_none() {
            spans.push(expr.span);
        }
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_invalid_try_targets_in_expression(resolution, programs, &binary.node.left, spans);
            collect_invalid_try_targets_in_expression(resolution, programs, &binary.node.right, spans);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_invalid_try_targets_in_expression(resolution, programs, &unary.node.expr, spans);
        }
        HirExpressionNode::CallExpression(call) => {
            collect_invalid_try_targets_in_expression(resolution, programs, &call.node.callee, spans);
            for arg in &call.node.args {
                collect_invalid_try_targets_in_expression(resolution, programs, arg, spans);
            }
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_invalid_try_targets_in_expression(resolution, programs, &match_expr.node.scrutinee, spans);
            for arm in &match_expr.node.arms {
                collect_invalid_try_targets_in_expression(resolution, programs, &arm.node.value, spans);
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_invalid_try_targets_in_block(resolution, programs, &block_expr.node.block, spans);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_invalid_try_targets_in_expression(resolution, programs, &grouped.node.expr, spans);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_invalid_try_targets_in_expression(resolution, programs, &assign.node.target, spans);
            collect_invalid_try_targets_in_expression(resolution, programs, &assign.node.value, spans);
        }
        _ => {}
    }
}

/// Map try-expression span → desugar metadata (computed before in-place normalization).
pub fn try_desugar_targets_for_program(
    resolution: &Resolution,
    entry: &Spanned<HirProgram>,
    dependency_programs: &[&Spanned<HirProgram>],
) -> HashMap<SpanInfo, TryDesugarTarget> {
    let mut programs: Vec<&Spanned<HirProgram>> = dependency_programs.to_vec();
    programs.push(entry);
    let mut map = HashMap::new();
    collect_try_targets(resolution, &programs, entry, &mut map);
    map
}

fn collect_try_targets(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    program: &Spanned<HirProgram>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    for item in &program.node.items {
        collect_try_targets_item(resolution, programs, item, map);
    }
}

fn collect_try_targets_item(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    item: &Spanned<HirItem>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            collect_try_targets_in_block(resolution, programs, &def.node.body, map);
        }
        HirItem::MethodDefinition(def) => {
            collect_try_targets_in_block(resolution, programs, &def.node.body, map);
        }
        HirItem::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_try_targets_item(resolution, programs, nested, map);
            }
        }
        _ => {}
    }
}

fn collect_try_targets_in_block(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    block: &Spanned<crate::hir::HirBlock>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    for statement in &block.node.statements {
        if let HirStatementNode::ExpressionStatement(expr_stmt) = &statement.node {
            collect_try_targets_in_expression(resolution, programs, &expr_stmt.node.expression, map);
        } else if let HirStatementNode::LetStatement(let_stmt) = &statement.node {
            collect_try_targets_in_expression(resolution, programs, &let_stmt.node.value, map);
        } else if let HirStatementNode::ReturnStatement(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value {
                collect_try_targets_in_expression(resolution, programs, value, map);
            }
    }
}

fn collect_try_targets_in_expression(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    expr: &Spanned<HirExpressionNode>,
    map: &mut HashMap<SpanInfo, TryDesugarTarget>,
) {
    if let HirExpressionNode::TryExpression(try_expr) = &expr.node
        && let Some(target) =
            try_desugar_target_for_operand(resolution, programs, &try_expr.node.expr)
        {
            map.insert(expr.span, target);
        }
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_try_targets_in_expression(resolution, programs, &binary.node.left, map);
            collect_try_targets_in_expression(resolution, programs, &binary.node.right, map);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_try_targets_in_expression(resolution, programs, &unary.node.expr, map);
        }
        HirExpressionNode::CallExpression(call) => {
            collect_try_targets_in_expression(resolution, programs, &call.node.callee, map);
            for arg in &call.node.args {
                collect_try_targets_in_expression(resolution, programs, arg, map);
            }
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_try_targets_in_expression(resolution, programs, &match_expr.node.scrutinee, map);
            for arm in &match_expr.node.arms {
                collect_try_targets_in_expression(resolution, programs, &arm.node.value, map);
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_try_targets_in_block(resolution, programs, &block_expr.node.block, map);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_try_targets_in_expression(resolution, programs, &grouped.node.expr, map);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_try_targets_in_expression(resolution, programs, &assign.node.target, map);
            collect_try_targets_in_expression(resolution, programs, &assign.node.value, map);
        }
        _ => {}
    }
}
