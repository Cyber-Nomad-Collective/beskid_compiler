//! Walk HIR bodies and discover call edges using [`TypeResult::call_kinds`].

use std::path::PathBuf;

use beskid_analysis::hir::{
    HirBlock, HirCallExpression, HirElseBranch, HirExpressionNode, HirStatementNode,
};
use beskid_analysis::paths::same_file;
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{CallLoweringKind, TypeId, TypeInfo, TypeResult};

use crate::lowering::function::{mangle_generic_item_function, mangle_method_name};
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
            collect_calls_in_block(
                &nested.node.then_block,
                resolution,
                type_result,
                source_path,
                out,
            );
            if let Some(nested_else) = &nested.node.else_branch {
                collect_calls_in_else_branch(
                    nested_else,
                    resolution,
                    type_result,
                    source_path,
                    out,
                );
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
            collect_calls_in_block(
                &if_stmt.node.then_block,
                resolution,
                type_result,
                source_path,
                out,
            );
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
            collect_calls_in_block(
                &while_stmt.node.body,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirStatementNode::ForStatement(for_stmt) => {
            collect_calls_in_expression(
                &for_stmt.node.iterable,
                resolution,
                type_result,
                source_path,
                out,
            );
            collect_calls_in_block(
                &for_stmt.node.body,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirStatementNode::ReturnStatement(ret) => {
            if let Some(value) = &ret.node.value {
                collect_calls_in_expression(value, resolution, type_result, source_path, out);
            }
        }
        HirStatementNode::LetStatement(let_stmt) => {
            collect_calls_in_expression(
                &let_stmt.node.value,
                resolution,
                type_result,
                source_path,
                out,
            );
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
            collect_calls_in_expression(
                &binary.node.left,
                resolution,
                type_result,
                source_path,
                out,
            );
            collect_calls_in_expression(
                &binary.node.right,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirExpressionNode::UnaryExpression(unary) => {
            collect_calls_in_expression(
                &unary.node.expr,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirExpressionNode::AssignExpression(assign) => {
            collect_calls_in_expression(
                &assign.node.target,
                resolution,
                type_result,
                source_path,
                out,
            );
            collect_calls_in_expression(
                &assign.node.value,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            collect_calls_in_expression(
                &grouped.node.expr,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirExpressionNode::BlockExpression(block_expr) => {
            collect_calls_in_block(
                &block_expr.node.block,
                resolution,
                type_result,
                source_path,
                out,
            );
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
                collect_calls_in_expression(
                    &arm.node.value,
                    resolution,
                    type_result,
                    source_path,
                    out,
                );
            }
        }
        HirExpressionNode::StructLiteralExpression(lit) => {
            for field in &lit.node.fields {
                collect_calls_in_expression(
                    &field.node.value,
                    resolution,
                    type_result,
                    source_path,
                    out,
                );
            }
        }
        HirExpressionNode::TryExpression(try_expr) => {
            collect_calls_in_expression(
                &try_expr.node.expr,
                resolution,
                type_result,
                source_path,
                out,
            );
        }
        HirExpressionNode::SpawnExpression(spawn) => {
            collect_spawn_entry_callees(
                &spawn.node.callee,
                resolution,
                source_path,
                out,
            );
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
            if let Some(ResolvedValue::Item(item_id)) = resolution
                .tables
                .resolved_value_at(path.node.path.span, source_path)
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

pub fn resolve_path_item_id(resolution: &Resolution, segments: &[String]) -> Option<ItemId> {
    item_id_from_module_graph(resolution, segments)
        .map(|item_id| canonical_item_id(resolution, item_id))
}

pub(crate) fn return_type_for_module_path_call(
    resolution: &Resolution,
    type_result: &TypeResult,
    call: &Spanned<HirCallExpression>,
) -> Option<TypeId> {
    let segments = path_segments_from_call(call)?;
    let item_id = canonical_item_id(resolution, item_id_from_module_graph(resolution, &segments)?);
    type_result
        .function_signatures
        .get(&item_id)
        .map(|signature| signature.return_type)
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
    let item_id = if let Some(ResolvedValue::Item(item_id)) = resolution
        .tables
        .resolved_value_at(callee_span, source_path)
    {
        item_id
    } else {
        item_id_for_call_path(resolution, call, source_path)?
    };
    Some(canonical_item_id(resolution, item_id))
}

fn receiver_item_for_type(type_result: &TypeResult, receiver_type: TypeId) -> Option<ItemId> {
    match type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => Some(*item_id),
        Some(TypeInfo::Applied { base, .. }) => Some(*base),
        _ => None,
    }
}

fn method_item_for_receiver_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    receiver_type: TypeId,
    method_name: &str,
) -> Option<ItemId> {
    let receiver_item = receiver_item_for_type(type_result, receiver_type)?;
    let receiver_name = resolution.items.get(receiver_item.0)?.name.as_str();
    let receiver_short = receiver_name.rsplit("::").next().unwrap_or(receiver_name);
    let qualified = format!("{receiver_short}::{method_name}");
    resolution
        .items
        .iter()
        .find(|info| {
            info.kind == ItemKind::Method
                && (info.name == qualified
                    || info.name.ends_with(&format!("::{method_name}")))
        })
        .map(|info| info.id)
}

fn resolve_member_method_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<ResolvedCall> {
    let HirExpressionNode::MemberExpression(member) = &call.node.callee.node else {
        return None;
    };
    let method_name = member.node.member.node.name.as_str();
    let receiver_type = type_result.expr_type(&member.node.target)?;
    let method_item_id = canonical_item_id(
        resolution,
        method_item_for_receiver_type(resolution, type_result, receiver_type, method_name)?,
    );
    let mangled = method_mangled_from_receiver(
        method_item_id,
        receiver_type,
        resolution,
        type_result,
    );
    Some(ResolvedCall {
        item_id: method_item_id,
        symbol: symbol_for_call(resolution, method_item_id),
        mangled,
        receiver_type: Some(receiver_type),
    })
}

fn resolve_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<ResolvedCall> {
    let kind = if let Some(kind) = type_result
        .lowering
        .call_kind_at(call.id)
        .copied()
        .map(|kind| crate::lowering::locals::canonicalize_call_kind(resolution, kind))
    {
        kind
    } else if let Some(resolved) =
        resolve_member_method_call(call, resolution, type_result, source_path)
    {
        return Some(resolved);
    } else if let Some(item_id) = resolve_item_call_id(call, resolution, source_path) {
        CallLoweringKind::ItemCall {
            item_id: canonical_item_id(resolution, item_id),
        }
    } else {
        return None;
    };
    match kind {
        CallLoweringKind::ItemCall { item_id } => {
            let item_id = canonical_item_id(resolution, item_id);
            let mut generic_args =
                generic_type_args_for_call(call, resolution, type_result, source_path);
            if generic_args.is_empty()
                && let Some(inferred) = infer_generic_type_args_for_call(
                    call,
                    item_id,
                    resolution,
                    type_result,
                    source_path,
                ) {
                    generic_args = inferred;
                }
            let mangled = build_function_mangled(item_id, &generic_args, resolution, type_result);
            Some(ResolvedCall {
                item_id,
                symbol: symbol_for_call(resolution, item_id),
                mangled,
                receiver_type: None,
            })
        }
        CallLoweringKind::MethodDispatch {
            method_item_id,
            receiver_type,
            ..
        } => {
            let method_item_id = canonical_item_id(resolution, method_item_id);
            let mangled = method_mangled_from_receiver(
                method_item_id,
                receiver_type,
                resolution,
                type_result,
            );
            Some(ResolvedCall {
                item_id: method_item_id,
                symbol: symbol_for_call(resolution, method_item_id),
                mangled,
                receiver_type: Some(receiver_type),
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

fn symbol_for_call(
    resolution: &Resolution,
    item_id: ItemId,
) -> Option<beskid_analysis::resolve::SymbolId> {
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

fn item_id_from_module_graph(resolution: &Resolution, segments: &[String]) -> Option<ItemId> {
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
            if let Some(path) = source_path
                && let Some(item) = many.iter().find(|item| {
                    resolution.items.get(item.0).is_some_and(|info| {
                        info.source_path
                            .as_ref()
                            .is_some_and(|source| beskid_analysis::paths::same_file(source, path))
                    })
                }) {
                    return Some(*item);
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
        mangled: method_mangled_from_receiver(
            method_item_id,
            receiver_type,
            resolution,
            type_result,
        ),
        receiver_type: Some(receiver_type),
    })
}

fn generic_type_args_for_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
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
        .filter_map(|arg| type_id_for_type(resolution, type_result, source_path, arg))
        .collect()
}

fn infer_generic_type_args_for_call(
    call: &Spanned<HirCallExpression>,
    item_id: ItemId,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<Vec<TypeId>> {
    let expected = type_result.generic_items.get(&item_id)?.len();
    if expected == 0 {
        return Some(Vec::new());
    }

    if let Some(expr_type) = type_result.node_type(call.id)
        && let Some(args) = infer_generic_args_from_call_expr_type(type_result, item_id, expr_type)
    {
        return Some(args);
    }

    let mut arg_types = Vec::with_capacity(call.node.args.len());
    for arg in &call.node.args {
        arg_types.push(expr_type_for_call_arg(
            arg,
            resolution,
            type_result,
            source_path,
        )?);
    }
    type_result.infer_generic_args_from_call_types(item_id, &arg_types)
}

fn infer_generic_args_from_call_expr_type(
    type_result: &TypeResult,
    item_id: ItemId,
    expr_type: TypeId,
) -> Option<Vec<TypeId>> {
    let generic_names = type_result.generic_items.get(&item_id)?;
    let expected = generic_names.len();
    if expected == 0 {
        return Some(Vec::new());
    }
    if let Some(TypeInfo::Applied { args, .. }) = type_result.types.get(expr_type)
        && args.len() == expected
    {
        return Some(args.clone());
    }
    let signature = type_result.function_signatures.get(&item_id)?;
    let mut mapping = std::collections::HashMap::new();
    if !bind_generic_args_from_return_type(
        &type_result.types,
        signature.return_type,
        expr_type,
        &mut mapping,
    ) || mapping.len() != expected
    {
        return None;
    }
    let mut substitution = Vec::with_capacity(expected);
    for name in generic_names {
        substitution.push(*mapping.get(name)?);
    }
    Some(substitution)
}

fn bind_generic_args_from_return_type(
    types: &beskid_analysis::types::TypeTable,
    param_type: TypeId,
    arg_type: TypeId,
    mapping: &mut std::collections::HashMap<String, TypeId>,
) -> bool {
    match types.get(param_type) {
        Some(TypeInfo::GenericParam(name)) => {
            if let Some(existing) = mapping.get(name) {
                *existing == arg_type
            } else {
                mapping.insert(name.clone(), arg_type);
                true
            }
        }
        Some(TypeInfo::Applied {
            base: param_base,
            args: param_args,
        }) => {
            let Some(TypeInfo::Applied {
                base: arg_base,
                args: arg_args,
            }) = types.get(arg_type)
            else {
                return false;
            };
            if param_base != arg_base || param_args.len() != arg_args.len() {
                return false;
            }
            for (param, arg) in param_args.iter().zip(arg_args.iter()) {
                if !bind_generic_args_from_return_type(types, *param, *arg, mapping) {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

fn expr_type_for_call_arg(
    arg: &Spanned<HirExpressionNode>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    if let Some(type_id) = type_result.node_type(arg.id) {
        return Some(type_id);
    }
    if let HirExpressionNode::PathExpression(path) = &arg.node {
        let span = path.node.path.span;
        if let Some(local_id) = resolution.tables.local_id_for_span(span, source_path)
            && let Some(type_id) = type_result.local_types.get(&local_id) {
                return Some(*type_id);
            }
    }
    None
}

fn build_function_mangled(
    item_id: ItemId,
    generic_args: &[TypeId],
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<String> {
    if generic_args.is_empty() {
        return None;
    }
    let base = resolution.items.get(item_id.0)?.name.clone();
    Some(mangle_generic_item_function(
        item_id,
        &base,
        generic_args,
        resolution,
        type_result,
    ))
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
