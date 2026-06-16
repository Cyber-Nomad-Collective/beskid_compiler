//! Local symbol lookup during lowering.

use std::path::PathBuf;

use beskid_analysis::hir::{HirExpressionNode, HirPrimitiveType};
use beskid_analysis::resolve::{HirNodeId, LocalId, Resolution, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{CallLoweringKind, TypeId, TypeInfo, TypeResult, field_type_on_receiver};

use crate::errors::CodegenError;

pub(crate) fn local_type_id(
    type_result: &TypeResult,
    state: &crate::lowering::function::FunctionLoweringState,
    local_id: LocalId,
) -> Option<TypeId> {
    state
        .local_type_overrides
        .get(&local_id)
        .copied()
        .or_else(|| type_result.local_types.get(&local_id).copied())
}

pub(crate) fn node_expr_type(type_result: &TypeResult, node_id: HirNodeId) -> Option<TypeId> {
    type_result.node_type(node_id)
}

pub(crate) fn expr_type_for_node(
    type_result: &TypeResult,
    node: &Spanned<HirExpressionNode>,
) -> Option<TypeId> {
    type_result.expr_type(node)
}

pub(crate) fn call_kind_for_call(
    type_result: &TypeResult,
    call: &Spanned<beskid_analysis::hir::HirCallExpression>,
) -> Option<CallLoweringKind> {
    type_result
        .lowering
        .call_kind_at(call.id)
        .copied()
}

pub(crate) fn require_expr_type(
    type_result: &TypeResult,
    node: &Spanned<HirExpressionNode>,
) -> Result<TypeId, CodegenError> {
    type_result
        .expr_type(node)
        .ok_or(CodegenError::MissingExpressionType { span: node.span })
}

pub(crate) fn primitive_type_id(
    type_result: &TypeResult,
    primitive: HirPrimitiveType,
) -> Option<TypeId> {
    type_result.types.find_primitive(primitive)
}

pub(crate) fn type_id_for_item(
    type_result: &TypeResult,
    item_id: beskid_analysis::resolve::ItemId,
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Named(id) if *id == item_id) {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn struct_literal_type_id(
    resolution: &Resolution,
    type_result: &TypeResult,
    path: &Spanned<beskid_analysis::hir::HirPath>,
    node_id: HirNodeId,
    _source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    if let Some(type_id) = type_result.node_type(node_id)
        && matches!(type_result.types.get(type_id), Some(TypeInfo::Named(_)))
    {
        return Some(type_id);
    }
    let segments: Vec<String> = path
        .node
        .segments
        .iter()
        .map(|segment| segment.node.name.node.name.clone())
        .collect();
    crate::lowering::types::resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
        .and_then(|item_id| type_id_for_item(type_result, item_id))
}

pub(crate) fn struct_field_type_for_receiver(
    resolution: &Resolution,
    type_result: &TypeResult,
    receiver_type: TypeId,
    field_name: &str,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    field_type_on_receiver(
        resolution,
        &type_result.path_env(),
        receiver_type,
        field_name,
        source_path,
    )
}

pub(crate) fn resolved_value_at(
    resolution: &Resolution,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<ResolvedValue> {
    resolution
        .tables
        .resolved_value_at(span, source_path)
        .map(|value| match value {
            ResolvedValue::Item(item_id) => {
                ResolvedValue::Item(canonical_item_id(resolution, item_id))
            }
            other => other,
        })
}

pub(crate) fn canonicalize_call_kind(
    resolution: &Resolution,
    kind: CallLoweringKind,
) -> CallLoweringKind {
    match kind {
        CallLoweringKind::ItemCall { item_id } => CallLoweringKind::ItemCall {
            item_id: canonical_item_id(resolution, item_id),
        },
        CallLoweringKind::MethodDispatch {
            method_item_id,
            receiver_source,
            receiver_type,
        } => CallLoweringKind::MethodDispatch {
            method_item_id: canonical_item_id(resolution, method_item_id),
            receiver_source,
            receiver_type,
        },
        CallLoweringKind::ContractDispatch {
            contract_item_id,
            receiver_source,
            receiver_type,
        } => CallLoweringKind::ContractDispatch {
            contract_item_id: canonical_item_id(resolution, contract_item_id),
            receiver_source,
            receiver_type,
        },
        other => other,
    }
}

pub(crate) fn local_id_for_span(
    resolution: &Resolution,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<LocalId> {
    if let Some(local_id) = resolution.tables.local_id_for_span(span, source_path) {
        return Some(local_id);
    }
    let Some(path) = source_path else {
        return None;
    };
    let matches: Vec<LocalId> = resolution
        .tables
        .locals
        .iter()
        .filter(|info| {
            info.span == span
                && info
                    .source_path
                    .as_ref()
                    .is_some_and(|local_path| beskid_analysis::paths::same_file(local_path, path))
        })
        .map(|info| info.id)
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}
