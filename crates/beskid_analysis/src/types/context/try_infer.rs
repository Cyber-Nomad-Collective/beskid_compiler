//! Infer enum metadata for `?` desugaring and array for-loop detection before full program type-check.

use std::collections::{HashMap, HashSet};

use crate::hir::{HirExpressionNode, HirItem, HirProgram, HirStatementNode};
use crate::resolve::Resolution;
use crate::syntax::{SpanInfo, Spanned};
use crate::types::TypeInfo;

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
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &let_stmt.node.value,
                spans,
            );
        } else if let HirStatementNode::ReturnStatement(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value
        {
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
        && try_desugar_target_for_operand(resolution, programs, &try_expr.node.expr).is_none()
    {
        spans.push(expr.span);
    }
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &binary.node.left,
                spans,
            );
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &binary.node.right,
                spans,
            );
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &unary.node.expr,
                spans,
            );
        }
        HirExpressionNode::CallExpression(call) => {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &call.node.callee,
                spans,
            );
            for arg in &call.node.args {
                collect_invalid_try_targets_in_expression(resolution, programs, arg, spans);
            }
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &match_expr.node.scrutinee,
                spans,
            );
            for arm in &match_expr.node.arms {
                collect_invalid_try_targets_in_expression(
                    resolution,
                    programs,
                    &arm.node.value,
                    spans,
                );
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_invalid_try_targets_in_block(
                resolution,
                programs,
                &block_expr.node.block,
                spans,
            );
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &grouped.node.expr,
                spans,
            );
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &assign.node.target,
                spans,
            );
            collect_invalid_try_targets_in_expression(
                resolution,
                programs,
                &assign.node.value,
                spans,
            );
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
            collect_try_targets_in_expression(
                resolution,
                programs,
                &expr_stmt.node.expression,
                map,
            );
        } else if let HirStatementNode::LetStatement(let_stmt) = &statement.node {
            collect_try_targets_in_expression(resolution, programs, &let_stmt.node.value, map);
        } else if let HirStatementNode::ReturnStatement(return_stmt) = &statement.node
            && let Some(value) = &return_stmt.node.value
        {
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
            collect_try_targets_in_expression(
                resolution,
                programs,
                &match_expr.node.scrutinee,
                map,
            );
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

/// Map for-statement span → true when the iterable type is `T[]` (computed before normalization).
pub fn collect_array_for_spans(
    resolution: &Resolution,
    entry: &Spanned<HirProgram>,
    dependency_programs: &[&Spanned<HirProgram>],
) -> HashSet<SpanInfo> {
    let mut programs: Vec<&Spanned<HirProgram>> = dependency_programs.to_vec();
    programs.push(entry);
    let mut set = HashSet::new();
    collect_array_fors(resolution, &programs, entry, &mut set);
    set
}

fn collect_array_fors(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    program: &Spanned<HirProgram>,
    set: &mut HashSet<SpanInfo>,
) {
    for item in &program.node.items {
        collect_array_fors_item(resolution, programs, item, set);
    }
}

fn collect_array_fors_item(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    item: &Spanned<HirItem>,
    set: &mut HashSet<SpanInfo>,
) {
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            collect_array_fors_in_block(resolution, programs, &def.node.body, set);
        }
        HirItem::MethodDefinition(def) => {
            collect_array_fors_in_block(resolution, programs, &def.node.body, set);
        }
        HirItem::InlineModule(inline) => {
            for nested in &inline.node.items {
                collect_array_fors_item(resolution, programs, nested, set);
            }
        }
        _ => {}
    }
}

fn collect_array_fors_in_else_branch(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    else_branch: &Spanned<crate::hir::HirElseBranch>,
    set: &mut HashSet<SpanInfo>,
) {
    match &else_branch.node {
        crate::hir::HirElseBranch::Block(block) => {
            collect_array_fors_in_block(resolution, programs, block, set);
        }
        crate::hir::HirElseBranch::If(nested) => {
            collect_array_fors_in_expression(resolution, programs, &nested.node.condition, set);
            collect_array_fors_in_block(resolution, programs, &nested.node.then_block, set);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_array_fors_in_else_branch(resolution, programs, nested_else, set);
            }
        }
    }
}

fn collect_array_fors_in_block(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    block: &Spanned<crate::hir::HirBlock>,
    set: &mut HashSet<SpanInfo>,
) {
    for statement in &block.node.statements {
        if let HirStatementNode::ForStatement(for_stmt) = &statement.node
            && is_array_iterable(resolution, programs, &for_stmt.node.iterable)
        {
            set.insert(statement.span);
        }
        collect_array_fors_in_statement(resolution, programs, statement, set);
    }
}

fn collect_array_fors_in_statement(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    statement: &Spanned<HirStatementNode>,
    set: &mut HashSet<SpanInfo>,
) {
    match &statement.node {
        HirStatementNode::LetStatement(let_stmt) => {
            collect_array_fors_in_expression(resolution, programs, &let_stmt.node.value, set);
        }
        HirStatementNode::ReturnStatement(ret) => {
            if let Some(value) = &ret.node.value {
                collect_array_fors_in_expression(resolution, programs, value, set);
            }
        }
        HirStatementNode::WhileStatement(while_stmt) => {
            collect_array_fors_in_expression(resolution, programs, &while_stmt.node.condition, set);
            collect_array_fors_in_block(resolution, programs, &while_stmt.node.body, set);
        }
        HirStatementNode::IfStatement(if_stmt) => {
            collect_array_fors_in_expression(resolution, programs, &if_stmt.node.condition, set);
            collect_array_fors_in_block(resolution, programs, &if_stmt.node.then_block, set);
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_array_fors_in_else_branch(resolution, programs, else_branch, set);
            }
        }
        HirStatementNode::ExpressionStatement(expr_stmt) => {
            collect_array_fors_in_expression(resolution, programs, &expr_stmt.node.expression, set);
        }
        HirStatementNode::ForStatement(_) => {}
        _ => {}
    }
}

fn collect_array_fors_in_expression(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    expr: &Spanned<HirExpressionNode>,
    set: &mut HashSet<SpanInfo>,
) {
    match &expr.node {
        HirExpressionNode::BinaryExpression(binary) => {
            collect_array_fors_in_expression(resolution, programs, &binary.node.left, set);
            collect_array_fors_in_expression(resolution, programs, &binary.node.right, set);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_array_fors_in_expression(resolution, programs, &unary.node.expr, set);
        }
        HirExpressionNode::CallExpression(call) => {
            collect_array_fors_in_expression(resolution, programs, &call.node.callee, set);
            for arg in &call.node.args {
                collect_array_fors_in_expression(resolution, programs, arg, set);
            }
        }
        HirExpressionNode::MemberExpression(member) => {
            collect_array_fors_in_expression(resolution, programs, &member.node.target, set);
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_array_fors_in_expression(resolution, programs, &match_expr.node.scrutinee, set);
            for arm in &match_expr.node.arms {
                collect_array_fors_in_expression(resolution, programs, &arm.node.value, set);
            }
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_array_fors_in_block(resolution, programs, &block_expr.node.block, set);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_array_fors_in_expression(resolution, programs, &grouped.node.expr, set);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_array_fors_in_expression(resolution, programs, &assign.node.target, set);
            collect_array_fors_in_expression(resolution, programs, &assign.node.value, set);
        }
        HirExpressionNode::IndexExpression(index_expr) => {
            collect_array_fors_in_expression(resolution, programs, &index_expr.node.target, set);
            collect_array_fors_in_expression(resolution, programs, &index_expr.node.index, set);
        }
        HirExpressionNode::ArrayLiteralExpression(lit) => {
            for element in &lit.node.elements {
                collect_array_fors_in_expression(resolution, programs, element, set);
            }
        }
        HirExpressionNode::EnumConstructorExpression(constructor) => {
            for arg in &constructor.node.args {
                collect_array_fors_in_expression(resolution, programs, arg, set);
            }
        }
        HirExpressionNode::StructLiteralExpression(struct_lit) => {
            for field in &struct_lit.node.fields {
                collect_array_fors_in_expression(resolution, programs, &field.node.value, set);
            }
        }
        HirExpressionNode::LambdaExpression(lambda) => {
            collect_array_fors_in_expression(resolution, programs, &lambda.node.body, set);
        }
        _ => {}
    }
}

fn is_array_iterable(
    resolution: &Resolution,
    programs: &[&Spanned<HirProgram>],
    iterable: &Spanned<HirExpressionNode>,
) -> bool {
    let mut ctx = TypeContext::new(resolution);
    ctx.seed_types();
    for program in programs {
        ctx.seed_enum_definitions(program);
    }
    let Some(target_type) = ctx.type_expression(iterable) else {
        return false;
    };
    matches!(ctx.type_table.get(target_type), Some(TypeInfo::Array(_)))
}
