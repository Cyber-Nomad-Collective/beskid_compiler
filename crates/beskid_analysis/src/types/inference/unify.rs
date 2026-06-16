//! Structural unification helpers for inference.

use crate::hir::HirPrimitiveType;
use crate::syntax::SpanInfo;
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo, TypeTable};

pub fn is_numeric(table: &TypeTable, type_id: TypeId) -> bool {
    matches!(
        table.get(type_id),
        Some(TypeInfo::Primitive(
            HirPrimitiveType::I32
                | HirPrimitiveType::I64
                | HirPrimitiveType::U8
                | HirPrimitiveType::F64
        ))
    )
}

pub fn unify_types(
    table: &TypeTable,
    left: TypeId,
    right: TypeId,
    span: SpanInfo,
) -> Result<TypeId, TypeError> {
    if left == right {
        return Ok(left);
    }

    if is_numeric(table, left) && is_numeric(table, right) {
        return unify_numeric_types(table, left, right).ok_or(TypeError::TypeMismatch {
            span,
            expected: left,
            actual: right,
        });
    }

    Err(TypeError::TypeMismatch {
        span,
        expected: left,
        actual: right,
    })
}

pub fn unify_numeric_types(table: &TypeTable, left: TypeId, right: TypeId) -> Option<TypeId> {
    if left == right {
        return Some(left);
    }
    if !is_numeric(table, left) || !is_numeric(table, right) {
        return None;
    }
    let left_prim = primitive_of(table, left)?;
    let right_prim = primitive_of(table, right)?;
    match (left_prim, right_prim) {
        (HirPrimitiveType::I64, HirPrimitiveType::I32) => Some(left),
        (HirPrimitiveType::I32, HirPrimitiveType::I64) => {
            table.find_primitive(HirPrimitiveType::I64)
        }
        _ => Some(left),
    }
}

fn primitive_of(table: &TypeTable, type_id: TypeId) -> Option<HirPrimitiveType> {
    match table.get(type_id)? {
        TypeInfo::Primitive(primitive) => Some(*primitive),
        _ => None,
    }
}
