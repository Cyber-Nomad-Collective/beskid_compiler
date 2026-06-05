//! Local symbol lookup during lowering.

use std::path::PathBuf;

use beskid_analysis::hir::{
    HirBinaryOp, HirCallExpression, HirExpressionNode, HirLiteral, HirPrimitiveType, HirUnaryOp,
};
use beskid_analysis::resolve::{canonical_item_id, LocalId, Resolution, ResolvedValue};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{CallLoweringKind, TypeId, TypeInfo, TypeResult};

use crate::errors::CodegenError;

pub(crate) fn expr_type_at(
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    type_result.expr_type_at(span, source_path)
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
        && let Some(type_id) = infer_expr_type(resolution, type_result, node, source_path)
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
        HirExpressionNode::PathExpression(path) => resolved_value_at(
            resolution,
            path.node.path.span,
            source_path,
        )
        .and_then(|resolved| match resolved {
            ResolvedValue::Local(local_id) => type_result.local_types.get(&local_id).copied(),
            _ => None,
        }),
        HirExpressionNode::BinaryExpression(binary) => {
            let left = infer_expr_type(resolution, type_result, &binary.node.left, source_path)?;
            let right = infer_expr_type(resolution, type_result, &binary.node.right, source_path)?;
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
            infer_expr_type(resolution, type_result, &grouped.node.expr, source_path)
        }
        HirExpressionNode::CallExpression(call) => {
            infer_call_expr_type(resolution, type_result, node, call, source_path)
        }
        HirExpressionNode::UnaryExpression(unary) => match unary.node.op.node {
            HirUnaryOp::Not => primitive_type_id(type_result, HirPrimitiveType::Bool),
            HirUnaryOp::Neg => {
                infer_expr_type(resolution, type_result, &unary.node.expr, source_path)
            }
        },
        _ => None,
    }
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
) -> Option<TypeId> {
    if let Some(kind) = type_result
        .call_kind_at(node.span, source_path)
        .map(|kind| canonicalize_call_kind(resolution, kind))
    {
        return infer_call_kind_return_type(resolution, type_result, &kind);
    }

    match &call.node.callee.node {
        HirExpressionNode::PathExpression(path) => {
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
                .function_signatures
                .get(&item_id)
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
    let value = resolution.tables.resolved_value_at(span, source_path)?;
    Some(match value {
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
    resolution.tables.local_id_for_span(span, source_path)
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
