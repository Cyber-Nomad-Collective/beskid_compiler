//! Serializable-type eligibility for AOT object-to-object mapping.
//!
//! v0.3 consults structural rules until Serialization Mod analyzers attach a dedicated signal on
//! [`TypeResult`]. Codegen must not emit mapping for ineligible pairs.

use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution};
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};

use crate::errors::CodegenError;

/// Well-known type name for the v0.3 dynamic cell surface (future language primitive).
pub const DYNAMIC_TYPE_NAME: &str = "dynamic";

fn item_name(resolution: &Resolution, item_id: ItemId) -> Option<&str> {
    resolution.items.iter().find(|item| item.id == item_id).map(|item| item.name.as_str())
}

fn is_serializable_field_type(type_result: &TypeResult, field_type: TypeId) -> bool {
    match type_result.types.get(field_type) {
        Some(TypeInfo::Primitive(primitive)) => matches!(
            primitive,
            HirPrimitiveType::Bool
                | HirPrimitiveType::I32
                | HirPrimitiveType::I64
                | HirPrimitiveType::U8
                | HirPrimitiveType::F64
                | HirPrimitiveType::Char
                | HirPrimitiveType::String
        ),
        Some(TypeInfo::Named(nested)) => is_serializable_struct_by_item(type_result, *nested),
        _ => false,
    }
}

fn is_serializable_struct_by_item(type_result: &TypeResult, item_id: ItemId) -> bool {
    let Some(fields) = type_result.struct_fields_ordered.get(&item_id) else {
        return false;
    };
    fields.iter().all(|(_, field_type)| is_serializable_field_type(type_result, *field_type))
}

/// Whether `item_id` names a struct eligible for `[Serialize]`-style mapping (structural stand-in).
pub fn is_serializable_struct(resolution: &Resolution, type_result: &TypeResult, item_id: ItemId) -> bool {
    let Some(info) = resolution.items.iter().find(|item| item.id == item_id) else {
        return false;
    };
    if info.kind != ItemKind::Type {
        return false;
    }
    is_serializable_struct_by_item(type_result, item_id)
}

/// Deterministic identity mapping when field names and types align in declaration order.
pub fn mapping_pair_eligible(resolution: &Resolution, type_result: &TypeResult, src: ItemId, dst: ItemId) -> bool {
    if !is_serializable_struct(resolution, type_result, src) || !is_serializable_struct(resolution, type_result, dst) {
        return false;
    }
    let Some(src_fields) = type_result.struct_fields_ordered.get(&src) else {
        return false;
    };
    let Some(dst_fields) = type_result.struct_fields_ordered.get(&dst) else {
        return false;
    };
    if src_fields.len() != dst_fields.len() {
        return false;
    }
    src_fields
        .iter()
        .zip(dst_fields.iter())
        .all(|((src_name, src_ty), (dst_name, dst_ty))| src_name == dst_name && src_ty == dst_ty)
}

/// Fail with a structured codegen diagnostic when mapping is not mod-eligible.
pub fn require_mapping_eligible(
    span: SpanInfo,
    resolution: &Resolution,
    type_result: &TypeResult,
    src: ItemId,
    dst: ItemId,
) -> Result<(), CodegenError> {
    if mapping_pair_eligible(resolution, type_result, src, dst) {
        return Ok(());
    }
    let src_name = item_name(resolution, src).unwrap_or("<unknown>");
    let dst_name = item_name(resolution, dst).unwrap_or("<unknown>");
    Err(CodegenError::IneligibleSerializeMapping {
        span,
        src_name: src_name.to_string(),
        dst_name: dst_name.to_string(),
    })
}

#[cfg(test)]
mod dynamic_serialize_eligibility_tests {
    use super::*;

    #[test]
    fn dynamic_type_name_is_stable() {
        assert_eq!(DYNAMIC_TYPE_NAME, "dynamic");
    }
}
