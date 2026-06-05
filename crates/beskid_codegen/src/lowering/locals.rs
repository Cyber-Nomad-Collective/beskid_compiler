//! Local symbol lookup during lowering.

use std::path::PathBuf;

use beskid_analysis::hir::{
    HirBinaryOp, HirCallExpression, HirExpressionNode, HirLiteral, HirMatchExpression,
    HirPath, HirPrimitiveType, HirUnaryOp,
};
use beskid_analysis::resolve::{canonical_item_id, ItemKind, LocalId, Resolution, ResolvedValue};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{CallLoweringKind, TypeId, TypeInfo, TypeResult};

use crate::errors::CodegenError;
use crate::linking::{resolve_item_call_id, resolve_path_item_id};

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
    if let Some(type_id) = type_result.expr_type_at(span, source_path) {
        return Some(type_id);
    }
    merge_scoped_expr_type(type_result, span, source_path, true).or_else(|| {
        if source_path.is_some() {
            None
        } else {
            merge_scoped_expr_type_unambiguous(type_result, span)
        }
    })
        .or_else(|| {
            if source_path.is_some() {
                None
            } else {
                type_result.expr_types.get(&span).copied()
            }
        })
}

fn merge_scoped_expr_type(
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
    restrict_to_source: bool,
) -> Option<TypeId> {
    let mut candidate: Option<TypeId> = None;
    for (scoped_path, types) in &type_result.scoped_expr_types {
        if restrict_to_source {
            let Some(path) = source_path else {
                continue;
            };
            if !beskid_analysis::paths::same_file(scoped_path, path) {
                continue;
            }
        }
        let Some(type_id) = types.get(&span).copied().or_else(|| {
            types
                .iter()
                .find(|(stored, _)| stored.start == span.start)
                .map(|(_, type_id)| *type_id)
        }) else {
            continue;
        };
        if let Some(existing) = candidate {
            if existing != type_id {
                return None;
            }
        } else {
            candidate = Some(type_id);
        }
    }
    candidate
}

fn merge_scoped_expr_type_unambiguous(
    type_result: &TypeResult,
    span: SpanInfo,
) -> Option<TypeId> {
    let mut unique: Option<TypeId> = None;
    for types in type_result.scoped_expr_types.values() {
        let Some(type_id) = types.get(&span).copied().or_else(|| {
            types
                .iter()
                .find(|(stored, _)| stored.start == span.start)
                .map(|(_, type_id)| *type_id)
        }) else {
            continue;
        };
        if let Some(existing) = unique {
            if existing != type_id {
                return None;
            }
        } else {
            unique = Some(type_id);
        }
    }
    unique
}

pub(crate) fn call_kind_at(
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<CallLoweringKind> {
    if let Some(kind) = type_result.call_kind_at(span, source_path) {
        return Some(kind);
    }
    merge_scoped_call_kind(type_result, span, source_path, true).or_else(|| {
        if source_path.is_some() {
            None
        } else {
            merge_scoped_call_kind_unambiguous(type_result, span)
        }
    })
        .or_else(|| {
            if source_path.is_some() {
                None
            } else {
                type_result.call_kinds.get(&span).copied()
            }
        })
}

fn merge_scoped_call_kind(
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
    restrict_to_source: bool,
) -> Option<CallLoweringKind> {
    let mut candidate: Option<CallLoweringKind> = None;
    for (scoped_path, kinds) in &type_result.scoped_call_kinds {
        if restrict_to_source {
            let Some(path) = source_path else {
                continue;
            };
            if !beskid_analysis::paths::same_file(scoped_path, path) {
                continue;
            }
        }
        let Some(kind) = kinds.get(&span).copied().or_else(|| {
            kinds
                .iter()
                .find(|(stored, _)| stored.start == span.start)
                .map(|(_, kind)| *kind)
        }) else {
            continue;
        };
        if let Some(existing) = candidate {
            if existing != kind {
                return None;
            }
        } else {
            candidate = Some(kind);
        }
    }
    candidate
}

fn merge_scoped_call_kind_unambiguous(
    type_result: &TypeResult,
    span: SpanInfo,
) -> Option<CallLoweringKind> {
    let mut unique: Option<CallLoweringKind> = None;
    for kinds in type_result.scoped_call_kinds.values() {
        let Some(kind) = kinds.get(&span).copied().or_else(|| {
            kinds
                .iter()
                .find(|(stored, _)| stored.start == span.start)
                .map(|(_, kind)| *kind)
        }) else {
            continue;
        };
        if let Some(existing) = unique {
            if existing != kind {
                return None;
            }
        } else {
            unique = Some(kind);
        }
    }
    unique
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
    if let Some(type_id) = expr_type_at(type_result, node.span, source_path) {
        return Some(type_id);
    }
    match &node.node {
        HirExpressionNode::LiteralExpression(literal) => match &literal.node.literal.node {
            HirLiteral::String(_) => primitive_type_id(type_result, HirPrimitiveType::String),
            HirLiteral::Integer(_) => primitive_type_id(type_result, HirPrimitiveType::I32),
            HirLiteral::Float(_) => primitive_type_id(type_result, HirPrimitiveType::F64),
            HirLiteral::Bool(_) => primitive_type_id(type_result, HirPrimitiveType::Bool),
            HirLiteral::Char(_) => primitive_type_id(type_result, HirPrimitiveType::Char),
            _ => None,
        },
        HirExpressionNode::MemberExpression(member) => {
            let target_type = infer_expr_type(
                resolution,
                type_result,
                &member.node.target,
                source_path,
                receiver_type,
            )?;
            struct_field_type_for_receiver(
                type_result,
                target_type,
                member.node.member.node.name.as_str(),
            )
        }
        HirExpressionNode::PathExpression(path) => {
            if path.node.path.node.segments.len() == 1 {
                let name = path.node.path.node.segments[0].node.name.node.name.as_str();
                if let Some(receiver_type) = receiver_type
                    && let Some(field_type) =
                        struct_field_type_for_receiver(type_result, receiver_type, name)
                {
                    return Some(field_type);
                }
            }
            if let Some(resolved) = resolved_value_at(
                resolution,
                path.node.path.span,
                source_path,
            ) && let ResolvedValue::Local(local_id) = resolved
            {
                return type_result.local_types.get(&local_id).copied();
            }
            if let Some(local_id) = resolution
                .tables
                .local_id_for_span(path.node.path.span, source_path)
                .or_else(|| local_id_for_span(resolution, path.node.path.span, source_path))
            {
                return type_result.local_types.get(&local_id).copied();
            }
            if path.node.path.node.segments.len() == 1 {
                let name = path.node.path.node.segments[0].node.name.node.name.as_str();
                if let Some(path) = source_path {
                    let scoped: Vec<_> = resolution
                        .tables
                        .locals
                        .iter()
                        .filter(|info| {
                            info.name == name
                                && info.source_path.as_ref().is_some_and(|local_path| {
                                    beskid_analysis::paths::same_file(local_path, path)
                                })
                        })
                        .collect();
                    if scoped.len() == 1 {
                        return type_result.local_types.get(&scoped[0].id).copied();
                    }
                }
            }
            None
        }
        HirExpressionNode::BinaryExpression(binary) => {
            let left = infer_expr_type(resolution, type_result, &binary.node.left, source_path, receiver_type)?;
            let right = infer_expr_type(resolution, type_result, &binary.node.right, source_path, receiver_type)?;
            match binary.node.op.node {
                HirBinaryOp::Add => {
                    if type_is_string(type_result, left) || type_is_string(type_result, right) {
                        return primitive_type_id(type_result, HirPrimitiveType::String)
                            .or(Some(left));
                    }
                    Some(left)
                }
                HirBinaryOp::And | HirBinaryOp::Or | HirBinaryOp::Eq | HirBinaryOp::NotEq
                | HirBinaryOp::Lt | HirBinaryOp::Lte | HirBinaryOp::Gt | HirBinaryOp::Gte
                | HirBinaryOp::IdentityEq | HirBinaryOp::IdentityNotEq => {
                    primitive_type_id(type_result, HirPrimitiveType::Bool)
                }
                HirBinaryOp::Sub | HirBinaryOp::Mul | HirBinaryOp::Div => Some(left),
            }
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            infer_expr_type(resolution, type_result, &grouped.node.expr, source_path, receiver_type)
        }
        HirExpressionNode::CallExpression(call) => {
            infer_call_expr_type(resolution, type_result, node, call, source_path, receiver_type)
        }
        HirExpressionNode::UnaryExpression(unary) => match unary.node.op.node {
            HirUnaryOp::Not => primitive_type_id(type_result, HirPrimitiveType::Bool),
            HirUnaryOp::Neg => {
                infer_expr_type(resolution, type_result, &unary.node.expr, source_path, receiver_type)
            }
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
        HirExpressionNode::MatchExpression(match_expr) => {
            infer_match_expr_type(resolution, type_result, match_expr, source_path, receiver_type)
        }
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

fn infer_match_expr_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    match_expr: &Spanned<HirMatchExpression>,
    source_path: Option<&PathBuf>,
    receiver_type: Option<TypeId>,
) -> Option<TypeId> {
    let _scrutinee =
        infer_expr_type(resolution, type_result, &match_expr.node.scrutinee, source_path, receiver_type)?;
    let mut expected: Option<TypeId> = None;
    for arm in &match_expr.node.arms {
        let arm_type = infer_expr_type(resolution, type_result, &arm.node.value, source_path, receiver_type);
        if let Some(actual) = arm_type {
            if let Some(expected_type) = expected {
                if actual != expected_type {
                    return expected;
                }
            } else {
                expected = Some(actual);
            }
        }
    }
    expected
}

pub(crate) fn type_id_for_item(type_result: &TypeResult, item_id: beskid_analysis::resolve::ItemId) -> Option<TypeId> {
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

pub(crate) fn resolve_type_path_item_id_for_codegen(
    resolution: &Resolution,
    type_result: &TypeResult,
    segments: &[String],
) -> Option<beskid_analysis::resolve::ItemId> {
    if let Some(item_id) = resolve_path_item_id(resolution, segments) {
        return Some(item_id);
    }
    let name = segments.last()?;
    for (item_id, type_name) in &type_result.named_type_names {
        if (type_name.as_str() == name.as_str() || type_name.ends_with(&format!("::{name}")))
            && (type_result.enum_variants_ordered.contains_key(item_id)
                || type_result.struct_fields_ordered.contains_key(item_id))
        {
            return Some(*item_id);
        }
    }
    resolution
        .items
        .iter()
        .find(|info| {
            matches!(info.kind, ItemKind::Type | ItemKind::Enum)
                && (info.name.as_str() == name.as_str() || info.name.ends_with(&format!("::{name}")))
        })
        .map(|info| info.id)
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
    if let HirExpressionNode::MemberExpression(member_expr) = &call.node.callee.node {
        let method_name = member_expr.node.member.node.name.as_str();
        let receiver_type = infer_expr_type(
            resolution,
            type_result,
            &member_expr.node.target,
            source_path,
            receiver_type,
        )?;
        if let Some(return_type) = method_return_type_for_receiver(
            resolution,
            type_result,
            receiver_type,
            method_name,
        ) {
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
            if segments.len() >= 2 {
                let method_name = segments[1].node.name.node.name.as_str();
                let local_id = resolved_value_at(
                    resolution,
                    path.node.path.span,
                    source_path,
                )
                .and_then(|resolved| match resolved {
                    ResolvedValue::Local(local_id) => Some(local_id),
                    _ => None,
                })
                .or_else(|| {
                    let receiver_name = segments[0].node.name.node.name.as_str();
                    resolution
                        .tables
                        .locals
                        .iter()
                        .find(|info| info.name == receiver_name)
                        .map(|info| info.id)
                });
                if let Some(local_id) = local_id
                    && let Some(receiver_type) = type_result.local_types.get(&local_id).copied()
                {
                    if let Some(return_type) = method_return_type_for_receiver(
                        resolution,
                        type_result,
                        receiver_type,
                        method_name,
                    ) {
                        return Some(return_type);
                    }
                }
            }
            let ResolvedValue::Item(item_id) = resolved_value_at(
                resolution,
                path.node.path.span,
                source_path,
            )?
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

fn struct_field_type_for_receiver(
    type_result: &TypeResult,
    receiver_type: TypeId,
    field_name: &str,
) -> Option<TypeId> {
    let item_id = match type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => return None,
    };
    type_result
        .struct_fields_ordered
        .get(&item_id)?
        .iter()
        .find(|(name, _)| name.as_str() == field_name)
        .map(|(_, field_type)| *field_type)
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
    if let Some(value) = resolution.tables.resolved_value_at(span, source_path) {
        return Some(match value {
            ResolvedValue::Item(item_id) => {
                ResolvedValue::Item(canonical_item_id(resolution, item_id))
            }
            other => other,
        });
    }
    if let Some(path) = source_path {
        let mut candidate: Option<ResolvedValue> = None;
        for (scoped_path, values) in &resolution.tables.scoped_resolved_values {
            if !beskid_analysis::paths::same_file(scoped_path, path) {
                continue;
            }
            let Some(value) = values.get(&span).copied().or_else(|| {
                values
                    .iter()
                    .find(|(stored, _)| stored.start == span.start)
                    .map(|(_, value)| *value)
            }) else {
                continue;
            };
            if let Some(existing) = candidate {
                if existing != value {
                    return None;
                }
            } else {
                candidate = Some(value);
            }
        }
        if let Some(value) = candidate {
            return Some(match value {
                ResolvedValue::Item(item_id) => {
                    ResolvedValue::Item(canonical_item_id(resolution, item_id))
                }
                other => other,
            });
        }
    }

    let mut unique: Option<ResolvedValue> = None;
    for values in resolution.tables.scoped_resolved_values.values() {
        let Some(value) = values.get(&span).copied() else {
            continue;
        };
        if let Some(existing) = unique {
            if existing != value {
                return None;
            }
        } else {
            unique = Some(value);
        }
    }
    unique.map(|value| match value {
        ResolvedValue::Item(item_id) => ResolvedValue::Item(canonical_item_id(resolution, item_id)),
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

pub(crate) fn source_path_for_item_span(
    resolution: &Resolution,
    span: SpanInfo,
) -> Option<PathBuf> {
    resolution
        .items
        .iter()
        .find(|info| info.span == span)
        .and_then(|info| info.source_path.clone())
}
