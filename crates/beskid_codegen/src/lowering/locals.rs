//! Local symbol lookup during lowering.

use std::path::PathBuf;

use beskid_analysis::hir::{HirCallExpression, HirExpressionNode, HirLiteral, HirPrimitiveType};
use beskid_analysis::resolve::{LocalId, Resolution, ResolvedValue};
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
    type_result: &TypeResult,
    span: SpanInfo,
    source_path: Option<&PathBuf>,
) -> Result<TypeId, CodegenError> {
    expr_type_at(type_result, span, source_path)
        .ok_or(CodegenError::MissingExpressionType { span })
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
            infer_expr_type(resolution, type_result, &binary.node.left, source_path)?;
            infer_expr_type(resolution, type_result, &binary.node.right, source_path)?;
            primitive_type_id(type_result, HirPrimitiveType::Bool)
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            infer_expr_type(resolution, type_result, &grouped.node.expr, source_path)
        }
        HirExpressionNode::CallExpression(call) => {
            infer_call_expr_type(resolution, type_result, node, call, source_path)
        }
        _ => None,
    }
}

fn infer_call_expr_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    node: &Spanned<HirExpressionNode>,
    call: &Spanned<HirCallExpression>,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    if let Some(kind) = type_result.call_kind_at(node.span, source_path) {
        return infer_call_kind_return_type(type_result, &kind);
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
            type_result
                .function_signatures
                .get(&item_id)
                .map(|signature| signature.return_type)
        }
        _ => None,
    }
}

fn infer_call_kind_return_type(
    type_result: &TypeResult,
    kind: &CallLoweringKind,
) -> Option<TypeId> {
    match kind {
        CallLoweringKind::ItemCall { item_id } => type_result
            .function_signatures
            .get(item_id)
            .map(|signature| signature.return_type),
        CallLoweringKind::MethodDispatch { method_item_id, .. } => type_result
            .function_signatures
            .get(method_item_id)
            .map(|signature| signature.return_type),
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
    resolution.tables.resolved_value_at(span, source_path)
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
