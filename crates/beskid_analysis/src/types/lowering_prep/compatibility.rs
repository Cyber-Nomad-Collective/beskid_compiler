use crate::hir::HirPrimitiveType;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::substitution::primitive_type_id;

pub(super) fn is_numeric(types: &TypeTable, id: TypeId) -> bool {
    matches!(
        types.get(id),
        Some(TypeInfo::Primitive(
            HirPrimitiveType::I32 | HirPrimitiveType::I64 | HirPrimitiveType::U8 | HirPrimitiveType::F64
        ))
    )
}

pub(super) fn is_never(types: &TypeTable, id: TypeId) -> bool {
    matches!(types.get(id), Some(TypeInfo::Primitive(HirPrimitiveType::Never)))
}

pub(super) fn literal_type_id(types: &TypeTable, lit: &crate::hir::HirLiteral) -> Option<TypeId> {
    use crate::hir::{HirLiteral, integer_literal_primitive_type};
    match lit {
        HirLiteral::Integer(v) => primitive_type_id(types, integer_literal_primitive_type(v)),
        HirLiteral::Float(_) => primitive_type_id(types, HirPrimitiveType::F64),
        HirLiteral::Bool(_) => primitive_type_id(types, HirPrimitiveType::Bool),
        HirLiteral::Char(_) => primitive_type_id(types, HirPrimitiveType::Char),
        HirLiteral::String(_) => primitive_type_id(types, HirPrimitiveType::String),
    }
}
