//! Walk HIR bodies and discover call edges using [`TypeResult::call_kinds`].

use std::path::PathBuf;

use beskid_analysis::hir::{
    HirBlock, HirCallExpression, HirElseBranch, HirExpressionNode, HirStatementNode,
};
use beskid_analysis::paths::same_file;
use beskid_analysis::resolve::{ItemId, ItemKind, ResolvedValue, Resolution, canonical_item_id};
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
            collect_calls_in_expression(
                &nested.node.condition,
                resolution,
                type_result,
                source_path,
                out,
            );
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
            if let Some(else_branch) = &if_stmt.node.else_branch {
                collect_calls_in_else_branch(
                    else_branch,
                    resolution,
                    type_result,
                    source_path,
                    out,
                );
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

pub fn resolve_path_item_id(
    resolution: &Resolution,
    segments: &[String],
) -> Option<ItemId> {
    item_id_from_module_graph(resolution, segments)
        .map(|item_id| canonical_item_id(resolution, item_id))
}

pub(crate) fn resolve_item_call_id(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    source_path: Option<&PathBuf>,
) -> Option<ItemId> {
    if let Some(segments) = path_segments_from_call(call)
        && let Some(item_id) = item_id_from_module_graph(resolution, &segments)
    {
        return Some(canonical_item_id(resolution, item_id));
    }

    let callee_span = match &call.node.callee.node {
        HirExpressionNode::PathExpression(path) => path.node.path.span,
        _ => call.node.callee.span,
    };
    let item_id = if let Some(ResolvedValue::Item(item_id)) =
        resolution.tables.resolved_value_at(callee_span, source_path)
    {
        item_id
    } else {
        item_id_for_call_path(resolution, call, source_path)?
    };
    Some(canonical_item_id(resolution, item_id))
}

fn resolve_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<ResolvedCall> {
    let kind = if let Some(item_id) = resolve_item_call_id(call, resolution, source_path) {
        CallLoweringKind::ItemCall {
            item_id: canonical_item_id(resolution, item_id),
        }
    } else if let Some(kind) = type_result
        .call_kind_at(call.span, source_path)
        .map(|kind| crate::lowering::locals::canonicalize_call_kind(resolution, kind))
    {
        kind
    } else {
        return None;
    };
    match kind {
        CallLoweringKind::ItemCall { item_id } => {
            let item_id = canonical_item_id(resolution, item_id);
            let generic_args = generic_type_args_for_call(call, resolution, type_result);
            let mangled = build_function_mangled(item_id, &generic_args, resolution);
            Some(ResolvedCall {
                item_id,
                symbol: symbol_for_call(resolution, item_id),
                mangled,
            })
        }
        CallLoweringKind::MethodDispatch {
            method_item_id,
            receiver_type,
            ..
        } => {
            let method_item_id = canonical_item_id(resolution, method_item_id);
            let mangled =
                method_mangled_from_receiver(method_item_id, receiver_type, resolution, type_result);
            Some(ResolvedCall {
                item_id: method_item_id,
                symbol: symbol_for_call(resolution, method_item_id),
                mangled,
            })
        }
        CallLoweringKind::ContractDispatch {
            contract_item_id,
            receiver_type,
            ..
        } => resolve_contract_dispatch_call(
            call,
            contract_item_id,
            receiver_type,
            resolution,
            type_result,
        ),
        CallLoweringKind::EventInvoke { .. } | CallLoweringKind::CallableValueCall => None,
    }
}

fn symbol_for_call(resolution: &Resolution, item_id: ItemId) -> Option<beskid_analysis::resolve::SymbolId> {
    resolution.items.get(item_id.0).and_then(|info| info.symbol)
}

fn path_segments_from_call(call: &Spanned<HirCallExpression>) -> Option<Vec<String>> {
    callee_path_segments(&call.node.callee)
}

fn callee_path_segments(callee: &Spanned<HirExpressionNode>) -> Option<Vec<String>> {
    match &callee.node {
        HirExpressionNode::PathExpression(path) => Some(
            path.node
                .path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.clone())
                .collect(),
        ),
        HirExpressionNode::MemberExpression(member) => {
            let mut segments = callee_path_segments(&member.node.target)?;
            segments.push(member.node.member.node.name.clone());
            Some(segments)
        }
        HirExpressionNode::GroupedExpression(grouped) => callee_path_segments(&grouped.node.expr),
        _ => None,
    }
}

fn item_id_from_module_graph(
    resolution: &Resolution,
    segments: &[String],
) -> Option<ItemId> {
    if segments.is_empty() {
        return None;
    }
    let name = segments.last()?;
    for module_path in candidate_module_paths(resolution, segments) {
        let Some(module_id) = resolution.module_graph.module_id(&module_path) else {
            continue;
        };
        let Some(module) = resolution.module_graph.module(module_id) else {
            continue;
        };
        if let Some(item_id) = module.scope.get(name) {
            return Some(*item_id);
        }
    }
    None
}

fn candidate_module_paths(resolution: &Resolution, segments: &[String]) -> Vec<Vec<String>> {
    if segments.len() < 2 {
        return Vec::new();
    }
    let prefix = &segments[..segments.len() - 1];
    let mut paths = vec![prefix.to_vec()];
    if let Some(import_target) = resolution.module_imports.get(&segments[0]) {
        let mut expanded = import_target.clone();
        expanded.extend_from_slice(&segments[1..segments.len() - 1]);
        paths.push(expanded);
    }
    if prefix.first().map(String::as_str) != Some("Platform") {
        let mut with_platform = vec!["Platform".to_string()];
        with_platform.extend_from_slice(prefix);
        paths.push(with_platform);
    }
    paths
}

fn item_id_for_call_path(
    resolution: &Resolution,
    call: &Spanned<HirCallExpression>,
    source_path: Option<&PathBuf>,
) -> Option<ItemId> {
    let segments = path_segments_from_call(call)?;
    if segments.len() == 1
        && let Some(path) = source_path
    {
        for info in &resolution.items {
            if !matches!(info.kind, ItemKind::Function | ItemKind::Method) {
                continue;
            }
            let display = info.name.rsplit("::").next().unwrap_or(info.name.as_str());
            if display != segments[0].as_str() {
                continue;
            }
            if info
                .source_path
                .as_ref()
                .is_some_and(|source| same_file(source, path))
            {
                return Some(info.id);
            }
        }
    }

    let name = segments.last()?;
    let module_suffix = if segments.len() > 1 {
        segments[..segments.len() - 1].join("::")
    } else {
        String::new()
    };
    let mut matches = Vec::new();
    for &item_id in resolution.by_symbol.values() {
        let Some(info) = resolution.items.get(item_id.0) else {
            continue;
        };
        if !matches!(info.kind, ItemKind::Function | ItemKind::Method) {
            continue;
        }
        let display = info.name.rsplit("::").next().unwrap_or(info.name.as_str());
        if display != name.as_str() {
            continue;
        }
        let Some(qn) = beskid_analysis::resolve::qualified_name(resolution, item_id) else {
            continue;
        };
        if !module_suffix.is_empty()
            && !qn.contains(&module_suffix)
            && !info.name.contains(&format!("::{module_suffix}::"))
        {
            continue;
        }
        matches.push(item_id);
    }
    match matches.as_slice() {
        [] => None,
        [single] => Some(*single),
        many => {
            if let Some(path) = source_path {
                if let Some(item) = many.iter().find(|item| {
                    resolution.items.get(item.0).is_some_and(|info| {
                        info.source_path
                            .as_ref()
                            .is_some_and(|source| beskid_analysis::paths::same_file(source, path))
                    })
                }) {
                    return Some(*item);
                }
            }
            many.last().copied()
        }
    }
}

fn contract_method_name(call: &Spanned<HirCallExpression>) -> Option<String> {
    match &call.node.callee.node {
        HirExpressionNode::PathExpression(path_expr) => path_expr
            .node
            .path
            .node
            .segments
            .last()
            .map(|segment| segment.node.name.node.name.clone()),
        HirExpressionNode::MemberExpression(member_expr) => {
            Some(member_expr.node.member.node.name.clone())
        }
        _ => None,
    }
}

fn resolve_contract_dispatch_call(
    call: &Spanned<HirCallExpression>,
    contract_item_id: ItemId,
    receiver_type: TypeId,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<ResolvedCall> {
    let method_name = contract_method_name(call)?;
    let receiver_item = match type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => contract_item_id,
    };
    let receiver_name = resolution.items.get(receiver_item.0)?.name.as_str();
    let expected = format!("{receiver_name}::{method_name}");
    let method_item_id = resolution
        .items
        .iter()
        .find(|info| info.kind == ItemKind::Method && info.name == expected)
        .or_else(|| {
            resolution.items.iter().find(|info| {
                info.kind == ItemKind::Method && info.name.ends_with(&format!("::{method_name}"))
            })
        })
        .map(|info| info.id)?;
    let method_item_id = canonical_item_id(resolution, method_item_id);
    Some(ResolvedCall {
        item_id: method_item_id,
        symbol: symbol_for_call(resolution, method_item_id),
        mangled: method_mangled_from_receiver(method_item_id, receiver_type, resolution, type_result),
    })
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
