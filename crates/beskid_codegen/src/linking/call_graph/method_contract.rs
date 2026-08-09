use std::path::PathBuf;

use beskid_analysis::hir::{HirCallExpression, HirExpressionNode};
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution, canonical_item_id};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{CallLoweringKind, TypeId, TypeInfo, TypeResult};

use crate::linking::plan::ResolvedCall;

use super::generics::{generic_type_args_for_call, infer_generic_type_args_for_call};
use super::path_resolution::resolve_item_call_id;
use super::symbols::{build_function_mangled, method_mangled_from_receiver, symbol_for_call};

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
                && (info.name == qualified || info.name.ends_with(&format!("::{method_name}")))
        })
        .map(|info| info.id)
}
pub(super) fn resolve_member_method_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    _source_path: Option<&PathBuf>,
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
    let mangled = method_mangled_from_receiver(method_item_id, receiver_type, resolution, type_result);
    Some(ResolvedCall {
        item_id: method_item_id,
        symbol: symbol_for_call(resolution, method_item_id),
        mangled,
        receiver_type: Some(receiver_type),
    })
}

fn contract_method_name(call: &Spanned<HirCallExpression>) -> Option<String> {
    match &call.node.callee.node {
        HirExpressionNode::PathExpression(path_expr) => {
            path_expr.node.path.node.segments.last().map(|segment| segment.node.name.node.name.clone())
        }
        HirExpressionNode::MemberExpression(member_expr) => Some(member_expr.node.member.node.name.clone()),
        _ => None,
    }
}

pub(super) fn resolve_contract_dispatch_call(
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
            resolution
                .items
                .iter()
                .find(|info| info.kind == ItemKind::Method && info.name.ends_with(&format!("::{method_name}")))
        })
        .map(|info| info.id)?;
    let method_item_id = canonical_item_id(resolution, method_item_id);
    Some(ResolvedCall {
        item_id: method_item_id,
        symbol: symbol_for_call(resolution, method_item_id),
        mangled: method_mangled_from_receiver(method_item_id, receiver_type, resolution, type_result),
        receiver_type: Some(receiver_type),
    })
}

pub(super) fn resolve_call(
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
    } else if let Some(resolved) = resolve_member_method_call(call, resolution, type_result, source_path) {
        return Some(resolved);
    } else if let Some(item_id) = resolve_item_call_id(call, resolution, source_path) {
        CallLoweringKind::ItemCall { item_id: canonical_item_id(resolution, item_id) }
    } else {
        return None;
    };
    match kind {
        CallLoweringKind::ItemCall { item_id } => {
            let item_id = canonical_item_id(resolution, item_id);
            let mut generic_args = generic_type_args_for_call(call, resolution, type_result, source_path);
            if generic_args.is_empty()
                && let Some(inferred) =
                    infer_generic_type_args_for_call(call, item_id, resolution, type_result, source_path)
            {
                generic_args = inferred;
            }
            let mangled = build_function_mangled(item_id, &generic_args, resolution, type_result);
            Some(ResolvedCall { item_id, symbol: symbol_for_call(resolution, item_id), mangled, receiver_type: None })
        }
        CallLoweringKind::MethodDispatch { method_item_id, receiver_type, .. } => {
            let method_item_id = canonical_item_id(resolution, method_item_id);
            let mangled = method_mangled_from_receiver(method_item_id, receiver_type, resolution, type_result);
            Some(ResolvedCall {
                item_id: method_item_id,
                symbol: symbol_for_call(resolution, method_item_id),
                mangled,
                receiver_type: Some(receiver_type),
            })
        }
        CallLoweringKind::ContractDispatch { contract_item_id, receiver_type, .. } => {
            resolve_contract_dispatch_call(call, contract_item_id, receiver_type, resolution, type_result)
        }
        CallLoweringKind::EventInvoke { .. } | CallLoweringKind::CallableValueCall => None,
    }
}
