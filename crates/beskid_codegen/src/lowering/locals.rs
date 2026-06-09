//! Local symbol lookup during lowering.

use std::path::PathBuf;

use beskid_analysis::hir::{
    HirBinaryOp, HirCallExpression, HirExpressionNode, HirLiteral, HirMatchExpression,
    HirPrimitiveType, HirUnaryOp,
};
use beskid_analysis::resolve::{ItemKind, LocalId, Resolution, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{
    CallLoweringKind, TypeId, TypeInfo, TypeResult, field_type_for_value_path,
    field_type_on_receiver, method_name_from_path_callee, receiver_type_for_path_callee,
    resolve_path_base_local,
};

use crate::errors::CodegenError;
use crate::linking::{
    resolve_item_call_id, return_type_for_module_path_call,
};
use crate::lowering::types::resolve_type_path_item_id_for_codegen;

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

pub(crate) fn expr_type_at(
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    type_result.expr_type_at(span, source_path)
}

pub(crate) fn call_kind_at(
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<CallLoweringKind> {
    type_result.call_kind_at(span, source_path)
}

pub(crate) fn require_expr_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
    node: Option<&Spanned<HirExpressionNode>>,
) -> Result<TypeId, CodegenError> {
    if let Some(type_id) = expr_type_at(type_result, span, source_path) {
        return Ok(type_id);
    }
    if let Some(node) = node
        && let Some(type_id) = infer_expr_type(resolution, type_result, node, source_path, None)
    {
        return Ok(type_id);
    }
    Err(CodegenError::MissingExpressionType { span })
}

pub(crate) fn primitive_type_id(
    type_result: &TypeResult,
    primitive: HirPrimitiveType,
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Primitive(found) if *found == primitive) {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn infer_expr_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    node: &Spanned<HirExpressionNode>,
    source_path: Option<&PathBuf>,
    receiver_type: Option<TypeId>,
) -> Option<TypeId> {
    // Prefer structural field lookup over span-keyed expr types, which can be
    // polluted when the same offsets appear across materialized compilation units.
    if let HirExpressionNode::MemberExpression(member) = &node.node {
        let target_type = infer_expr_type(
            resolution,
            type_result,
            &member.node.target,
            source_path,
            receiver_type,
        )?;
        return struct_field_type_for_receiver(
            resolution,
            type_result,
            target_type,
            member.node.member.node.name.as_str(),
            source_path,
        );
    }

    if !matches!(
        &node.node,
        HirExpressionNode::PathExpression(_)
            | HirExpressionNode::MemberExpression(_)
            | HirExpressionNode::LiteralExpression(_)
    ) && let Some(type_id) = expr_type_at(type_result, node.span, source_path)
    {
        return Some(type_id);
    }
    match &node.node {
        HirExpressionNode::LiteralExpression(literal) => match &literal.node.literal.node {
            HirLiteral::String(_) => primitive_type_id(type_result, HirPrimitiveType::String),
            HirLiteral::Integer(text) => primitive_type_id(
                type_result,
                beskid_analysis::hir::integer_literal_primitive_type(text),
            ),
            HirLiteral::Float(_) => primitive_type_id(type_result, HirPrimitiveType::F64),
            HirLiteral::Bool(_) => primitive_type_id(type_result, HirPrimitiveType::Bool),
            HirLiteral::Char(_) => primitive_type_id(type_result, HirPrimitiveType::Char),
        },
        HirExpressionNode::PathExpression(path) => {
            let segments = &path.node.path.node.segments;
            if segments.is_empty() {
                return None;
            }
            if segments.len() == 1 {
                let name = segments[0].node.name.node.name.as_str();
                if let Some(receiver_type) = receiver_type
                    && let Some(field_type) = struct_field_type_for_receiver(
                        resolution,
                        type_result,
                        receiver_type,
                        name,
                        source_path,
                    )
                {
                    return Some(field_type);
                }
            }
            if segments.len() >= 2
                && let Some(type_id) = field_type_for_value_path(
                    resolution,
                    &type_result.path_env(),
                    path.node.path.span,
                    &path.node.path,
                    source_path,
                )
            {
                return Some(type_id);
            }
            if segments.len() >= 2 {
                return None;
            }
            let first_name = segments[0].node.name.node.name.as_str();
            if let Some(local_id) =
                resolve_path_base_local(resolution, path.node.path.span, first_name, source_path)
            {
                return type_result.local_types.get(&local_id).copied();
            }
            expr_type_at(type_result, node.span, source_path)
        }
        HirExpressionNode::BinaryExpression(binary) => {
            let left = infer_expr_type(
                resolution,
                type_result,
                &binary.node.left,
                source_path,
                receiver_type,
            );
            let right = infer_expr_type(
                resolution,
                type_result,
                &binary.node.right,
                source_path,
                receiver_type,
            );
            match binary.node.op.node {
                HirBinaryOp::Add => {
                    if left.is_some_and(|type_id| type_is_string(type_result, type_id))
                        || right.is_some_and(|type_id| type_is_string(type_result, type_id))
                    {
                        return primitive_type_id(type_result, HirPrimitiveType::String)
                            .or(left)
                            .or(right);
                    }
                    left.or(right)
                }
                HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Eq
                | HirBinaryOp::NotEq
                | HirBinaryOp::Lt
                | HirBinaryOp::Lte
                | HirBinaryOp::Gt
                | HirBinaryOp::Gte
                | HirBinaryOp::IdentityEq
                | HirBinaryOp::IdentityNotEq => {
                    primitive_type_id(type_result, HirPrimitiveType::Bool)
                }
                HirBinaryOp::Sub | HirBinaryOp::Mul | HirBinaryOp::Div | HirBinaryOp::Mod => {
                    left.or(right)
                }
            }
        }
        HirExpressionNode::GroupedExpression(grouped) => infer_expr_type(
            resolution,
            type_result,
            &grouped.node.expr,
            source_path,
            receiver_type,
        ),
        HirExpressionNode::CallExpression(call) => infer_call_expr_type(
            resolution,
            type_result,
            node,
            call,
            source_path,
            receiver_type,
        ),
        HirExpressionNode::UnaryExpression(unary) => match unary.node.op.node {
            HirUnaryOp::Not => primitive_type_id(type_result, HirPrimitiveType::Bool),
            HirUnaryOp::Neg => infer_expr_type(
                resolution,
                type_result,
                &unary.node.expr,
                source_path,
                receiver_type,
            ),
        },
        HirExpressionNode::EnumConstructorExpression(constructor) => {
            let segments: Vec<String> = constructor
                .node
                .path
                .node
                .type_path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.clone())
                .collect();
            resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
                .and_then(|item_id| type_id_for_item(type_result, item_id))
        }
        HirExpressionNode::MatchExpression(match_expr) => infer_match_expr_type(
            resolution,
            type_result,
            match_expr,
            source_path,
            receiver_type,
        ),
        HirExpressionNode::StructLiteralExpression(lit) => struct_literal_type_id(
            resolution,
            type_result,
            &lit.node.path,
            node.span,
            source_path,
        ),
        _ => None,
    }
}

pub(crate) fn infer_match_expr_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    match_expr: &Spanned<HirMatchExpression>,
    source_path: Option<&PathBuf>,
    receiver_type: Option<TypeId>,
) -> Option<TypeId> {
    let _scrutinee = infer_expr_type(
        resolution,
        type_result,
        &match_expr.node.scrutinee,
        source_path,
        receiver_type,
    )?;
    let mut expected: Option<TypeId> = None;
    for arm in &match_expr.node.arms {
        let arm_type = infer_expr_type(
            resolution,
            type_result,
            &arm.node.value,
            source_path,
            receiver_type,
        );
        if let Some(actual) = arm_type {
            if let Some(expected_type) = expected {
                if actual == expected_type {
                    continue;
                }
                if is_numeric(type_result, actual) && is_numeric(type_result, expected_type) {
                    expected = preferred_numeric_type_id(type_result, expected_type, actual);
                    continue;
                }
                return expected;
            } else {
                expected = Some(actual);
            }
        }
    }
    expected
}

fn is_numeric(type_result: &TypeResult, type_id: TypeId) -> bool {
    matches!(
        type_result.types.get(type_id),
        Some(TypeInfo::Primitive(
            HirPrimitiveType::I32
                | HirPrimitiveType::I64
                | HirPrimitiveType::U8
                | HirPrimitiveType::F64
        ))
    )
}

fn preferred_numeric_type_id(
    type_result: &TypeResult,
    left: TypeId,
    right: TypeId,
) -> Option<TypeId> {
    let width = |type_id: TypeId| {
        type_result
            .types
            .get(type_id)
            .and_then(|info| match info {
                TypeInfo::Primitive(primitive) => Some(primitive.bit_width()),
                _ => None,
            })
            .unwrap_or(0)
    };
    if width(left) >= width(right) { Some(left) } else { Some(right) }
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
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    if let Some(type_id) = expr_type_at(type_result, span, source_path)
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
    resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
        .and_then(|item_id| type_id_for_item(type_result, item_id))
}

fn type_is_string(type_result: &TypeResult, type_id: TypeId) -> bool {
    matches!(
        type_result.types.get(type_id),
        Some(TypeInfo::Primitive(HirPrimitiveType::String))
    )
}

fn infer_call_expr_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    node: &Spanned<HirExpressionNode>,
    call: &Spanned<HirCallExpression>,
    source_path: Option<&PathBuf>,
    receiver_type: Option<TypeId>,
) -> Option<TypeId> {
    if let Some(return_type) = return_type_for_module_path_call(resolution, type_result, call) {
        return Some(return_type);
    }

    if let HirExpressionNode::MemberExpression(member_expr) = &call.node.callee.node {
        let method_name = member_expr.node.member.node.name.as_str();
        let receiver_type = infer_expr_type(
            resolution,
            type_result,
            &member_expr.node.target,
            source_path,
            receiver_type,
        )?;
        if let Some(return_type) =
            method_return_type_for_receiver(resolution, type_result, receiver_type, method_name)
        {
            return Some(return_type);
        }
    }

    if let Some(kind) = call_kind_at(type_result, node.span, source_path)
        .map(|kind| canonicalize_call_kind(resolution, kind))
    {
        return infer_call_kind_return_type(resolution, type_result, &kind);
    }

    if let Some(item_id) = resolve_item_call_id(call, resolution, source_path) {
        return type_result
            .function_signatures
            .get(&item_id)
            .map(|signature| signature.return_type);
    }

    match &call.node.callee.node {
        HirExpressionNode::MemberExpression(_) => None,
        HirExpressionNode::PathExpression(path) => {
            let segments = &path.node.path.node.segments;
            if segments.len() >= 2
                && let Some(method_name) = method_name_from_path_callee(segments)
                && let Some((_local_id, receiver_type)) = receiver_type_for_path_callee(
                    resolution,
                    &type_result.path_env(),
                    path.node.path.span,
                    segments,
                    source_path,
                )
                && let Some(return_type) = method_return_type_for_receiver(
                    resolution,
                    type_result,
                    receiver_type,
                    method_name,
                )
            {
                return Some(return_type);
            }
            let ResolvedValue::Item(item_id) =
                resolved_value_at(resolution, path.node.path.span, source_path)?
            else {
                return None;
            };
            let item_id = canonical_item_id(resolution, item_id);
            type_result
                .function_signatures
                .get(&item_id)
                .map(|signature| signature.return_type)
        }
        _ => None,
    }
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

fn method_return_type_for_receiver(
    resolution: &Resolution,
    type_result: &TypeResult,
    receiver_type: TypeId,
    method_name: &str,
) -> Option<TypeId> {
    method_item_for_receiver_type(resolution, type_result, receiver_type, method_name).and_then(
        |method_item_id| {
            let item_id = canonical_item_id(resolution, method_item_id);
            type_result
                .method_function_signatures
                .get(&item_id)
                .or_else(|| type_result.function_signatures.get(&item_id))
                .map(|signature| signature.return_type)
        },
    )
}

fn method_item_for_receiver_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    receiver_type: TypeId,
    method_name: &str,
) -> Option<beskid_analysis::resolve::ItemId> {
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
    let expected = format!("{receiver_name}::{method_name}");
    resolution
        .items
        .iter()
        .find(|info| {
            info.kind == ItemKind::Method
                && (info.name == expected || info.name.ends_with(&format!("::{method_name}")))
        })
        .map(|info| info.id)
}

fn infer_call_kind_return_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    kind: &CallLoweringKind,
) -> Option<TypeId> {
    match kind {
        CallLoweringKind::ItemCall { item_id } => {
            let item_id = canonical_item_id(resolution, *item_id);
            type_result
                .function_signatures
                .get(&item_id)
                .map(|signature| signature.return_type)
        }
        CallLoweringKind::MethodDispatch { method_item_id, .. } => {
            let item_id = canonical_item_id(resolution, *method_item_id);
            type_result
                .method_function_signatures
                .get(&item_id)
                .or_else(|| type_result.function_signatures.get(&item_id))
                .map(|signature| signature.return_type)
        }
        CallLoweringKind::EventInvoke { .. } => {
            primitive_type_id(type_result, HirPrimitiveType::Unit)
        }
        CallLoweringKind::ContractDispatch { .. } | CallLoweringKind::CallableValueCall => None,
    }
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
