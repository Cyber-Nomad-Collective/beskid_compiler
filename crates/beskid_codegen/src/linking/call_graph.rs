//! Walk HIR bodies and discover call edges using [`TypeResult::call_kinds`].

use std::path::PathBuf;

use beskid_analysis::hir::{
    HirBlock, HirCallExpression, HirExpressionNode, HirStatementNode,
};
use beskid_analysis::resolve::{ItemId, Resolution};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{CallLoweringKind, TypeId, TypeInfo, TypeResult};

use crate::lowering::function::{mangle_function_name, mangle_method_name};
use crate::lowering::types::type_id_for_type;

use super::plan::ResolvedCall;

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

fn collect_calls_in_statement(
    statement: &Spanned<HirStatementNode>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
    out: &mut Vec<ResolvedCall>,
) {
    match &statement.node {
        HirStatementNode::ExpressionStatement(expr_stmt) => {
            collect_calls_in_expression(
                &expr_stmt.node.expression,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirStatementNode::IfStatement(if_stmt) => {
            collect_calls_in_expression(
                &if_stmt.node.condition,
                resolution,
                type_result,
                source_path,
                out,
            );
            collect_calls_in_block(&if_stmt.node.then_block, resolution, type_result, source_path, out);
            if let Some(else_block) = &if_stmt.node.else_block {
                collect_calls_in_block(else_block, resolution, type_result, source_path, out);
            }
        }
        HirStatementNode::WhileStatement(while_stmt) => {
            collect_calls_in_expression(
                &while_stmt.node.condition,
                resolution,
                type_result,
                source_path,
                out,
            );
            collect_calls_in_block(&while_stmt.node.body, resolution, type_result, source_path, out);
        }
        HirStatementNode::ForStatement(for_stmt) => {
            collect_calls_in_expression(
                &for_stmt.node.iterable,
                resolution,
                type_result,
                source_path,
                out,
            );
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
            collect_calls_in_expression(
                &match_expr.node.scrutinee,
                resolution,
                type_result,
                source_path,
                out,
            );
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
            collect_calls_in_expression(&try_expr.node.expr, resolution, type_result, source_path, out);
        }
        _ => {}
    }
}

fn resolve_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<ResolvedCall> {
    let kind = type_result.call_kind_at(call.span, source_path)?;
    match kind {
        CallLoweringKind::ItemCall { item_id } => {
            let generic_args = generic_type_args_for_call(call, resolution, type_result);
            let mangled = build_function_mangled(item_id, &generic_args, resolution);
            Some(ResolvedCall { item_id, mangled })
        }
        CallLoweringKind::MethodDispatch {
            method_item_id,
            receiver_type,
            ..
        } => {
            let mangled =
                method_mangled_from_receiver(method_item_id, receiver_type, resolution, type_result);
            Some(ResolvedCall {
                item_id: method_item_id,
                mangled,
            })
        }
        CallLoweringKind::ContractDispatch { .. }
        | CallLoweringKind::EventInvoke { .. }
        | CallLoweringKind::CallableValueCall => None,
    }
}

fn generic_type_args_for_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Vec<TypeId> {
    let HirExpressionNode::PathExpression(path_expr) = &call.node.callee.node else {
        return Vec::new();
    };
    let Some(segment) = path_expr.node.path.node.segments.last() else {
        return Vec::new();
    };
    segment
        .node
        .type_args
        .iter()
        .filter_map(|arg| type_id_for_type(resolution, type_result, arg))
        .collect()
}

fn build_function_mangled(
    item_id: ItemId,
    generic_args: &[TypeId],
    resolution: &Resolution,
) -> Option<String> {
    if generic_args.is_empty() {
        return None;
    }
    let base = resolution.items.get(item_id.0)?.name.clone();
    Some(mangle_function_name(&base, generic_args))
}

fn method_mangled_from_receiver(
    method_item_id: ItemId,
    receiver_type: TypeId,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<String> {
    let method_name = resolution.items.get(method_item_id.0)?.name.as_str();
    let receiver_item = match type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => return None,
    };
    let receiver_name = resolution
        .items
        .iter()
        .find(|info| info.id == receiver_item)
        .map(|info| info.name.as_str())?;
    Some(mangle_method_name(receiver_name, method_name))
}
