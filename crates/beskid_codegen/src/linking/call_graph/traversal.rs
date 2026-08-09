use std::path::PathBuf;

use beskid_analysis::hir::{HirBlock, HirElseBranch, HirExpressionNode, HirStatementNode};
use beskid_analysis::resolve::{Resolution, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeResult;

use crate::linking::plan::ResolvedCall;

use super::method_contract::resolve_call;
use super::symbols::symbol_for_call;

pub(crate) fn collect_calls_in_body(
    body: &Spanned<HirBlock>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Vec<ResolvedCall> {
    let mut out = Vec::new();
    for statement in &body.node.statements {
        collect_calls_in_statement(statement, resolution, type_result, source_path, &mut out);
    }
    out
}
fn collect_calls_in_else_branch(
    else_branch: &Spanned<HirElseBranch>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
    out: &mut Vec<ResolvedCall>,
) {
    match &else_branch.node {
        HirElseBranch::Block(block) => {
            collect_calls_in_block(block, resolution, type_result, source_path, out);
        }
        HirElseBranch::If(nested) => {
            collect_calls_in_expression(&nested.node.condition, resolution, type_result, source_path, out);
            collect_calls_in_block(&nested.node.then_block, resolution, type_result, source_path, out);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_calls_in_else_branch(nested_else, resolution, type_result, source_path, out);
            }
        }
    }
}

fn collect_calls_in_statement(
    statement: &Spanned<HirStatementNode>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
    out: &mut Vec<ResolvedCall>,
) {
    match &statement.node {
        HirStatementNode::ExpressionStatement(expr_stmt) => {
            collect_calls_in_expression(&expr_stmt.node.expression, resolution, type_result, source_path, out);
        }
        HirStatementNode::IfStatement(if_stmt) => {
            collect_calls_in_expression(&if_stmt.node.condition, resolution, type_result, source_path, out);
            collect_calls_in_block(&if_stmt.node.then_block, resolution, type_result, source_path, out);
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_calls_in_else_branch(else_branch, resolution, type_result, source_path, out);
            }
        }
        HirStatementNode::WhileStatement(while_stmt) => {
            collect_calls_in_expression(&while_stmt.node.condition, resolution, type_result, source_path, out);
            collect_calls_in_block(&while_stmt.node.body, resolution, type_result, source_path, out);
        }
        HirStatementNode::ForStatement(for_stmt) => {
            collect_calls_in_expression(&for_stmt.node.iterable, resolution, type_result, source_path, out);
            collect_calls_in_block(&for_stmt.node.body, resolution, type_result, source_path, out);
        }
        HirStatementNode::ReturnStatement(ret) => {
            if let Some(value) = &ret.node.value {
                collect_calls_in_expression(value, resolution, type_result, source_path, out);
            }
        }
        HirStatementNode::LetStatement(let_stmt) => {
            collect_calls_in_expression(&let_stmt.node.value, resolution, type_result, source_path, out);
        }
        _ => {}
    }
}

fn collect_calls_in_block(
    block: &Spanned<HirBlock>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
    out: &mut Vec<ResolvedCall>,
) {
    for statement in &block.node.statements {
        collect_calls_in_statement(statement, resolution, type_result, source_path, out);
    }
}

fn collect_calls_in_expression(
    expr: &Spanned<HirExpressionNode>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
    out: &mut Vec<ResolvedCall>,
) {
    match &expr.node {
        HirExpressionNode::CallExpression(call) => {
            if let Some(resolved) = resolve_call(call, resolution, type_result, source_path) {
                out.push(resolved);
            }
            for arg in &call.node.args {
                collect_calls_in_expression(arg, resolution, type_result, source_path, out);
            }
        }
        HirExpressionNode::BinaryExpression(binary) => {
            collect_calls_in_expression(&binary.node.left, resolution, type_result, source_path, out);
            collect_calls_in_expression(&binary.node.right, resolution, type_result, source_path, out);
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_calls_in_expression(&unary.node.expr, resolution, type_result, source_path, out);
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_calls_in_expression(&assign.node.target, resolution, type_result, source_path, out);
            collect_calls_in_expression(&assign.node.value, resolution, type_result, source_path, out);
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_calls_in_expression(&grouped.node.expr, resolution, type_result, source_path, out);
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_calls_in_block(&block_expr.node.block, resolution, type_result, source_path, out);
        }
        HirExpressionNode::MatchExpression(match_expr) => {
            collect_calls_in_expression(&match_expr.node.scrutinee, resolution, type_result, source_path, out);
            for arm in &match_expr.node.arms {
                if let Some(guard) = &arm.node.guard {
                    collect_calls_in_expression(guard, resolution, type_result, source_path, out);
                }
                collect_calls_in_expression(&arm.node.value, resolution, type_result, source_path, out);
            }
        }
        HirExpressionNode::StructLiteralExpression(lit) => {
            for field in &lit.node.fields {
                collect_calls_in_expression(&field.node.value, resolution, type_result, source_path, out);
            }
        }
        HirExpressionNode::TryExpression(try_expr) => {
            collect_calls_in_expression(&try_expr.node.body, resolution, type_result, source_path, out);
        }
        HirExpressionNode::SpawnExpression(spawn) => {
            collect_spawn_entry_callees(&spawn.node.callee, resolution, source_path, out);
        }
        _ => {}
    }
}

fn collect_spawn_entry_callees(
    callee: &Spanned<HirExpressionNode>,
    resolution: &Resolution,
    source_path: Option<&PathBuf>,
    out: &mut Vec<ResolvedCall>,
) {
    match &callee.node {
        HirExpressionNode::PathExpression(path) => {
            if let Some(ResolvedValue::Item(item_id)) =
                resolution.tables.resolved_value_at(path.node.path.span, source_path)
            {
                let item_id = canonical_item_id(resolution, item_id);
                out.push(ResolvedCall {
                    item_id,
                    symbol: symbol_for_call(resolution, item_id),
                    mangled: None,
                    receiver_type: None,
                });
            }
        }
        HirExpressionNode::CallExpression(call) if call.node.args.is_empty() => {
            collect_spawn_entry_callees(&call.node.callee, resolution, source_path, out);
        }
        HirExpressionNode::LambdaExpression(_) => {}
        _ => {}
    }
}
