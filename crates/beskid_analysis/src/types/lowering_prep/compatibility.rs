use crate::syntax::PrimitiveType;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::substitution::primitive_type_id;

pub(super) fn is_numeric(types: &TypeTable, id: TypeId) -> bool {
    matches!(
        types.get(id),
        Some(TypeInfo::Primitive(PrimitiveType::I32 | PrimitiveType::I64 | PrimitiveType::U8 | PrimitiveType::F64))
    )
}

pub(super) fn is_never(types: &TypeTable, id: TypeId) -> bool {
    matches!(types.get(id), Some(TypeInfo::Primitive(PrimitiveType::Never)))
}

pub(super) fn literal_type_id(types: &TypeTable, lit: &crate::syntax::Literal) -> Option<TypeId> {
    use crate::syntax::{Literal, integer_literal_primitive_type};
    match lit {
        Literal::Integer(v) => primitive_type_id(types, integer_literal_primitive_type(v)),
        Literal::Float(_) => primitive_type_id(types, PrimitiveType::F64),
        Literal::Bool(_) => primitive_type_id(types, PrimitiveType::Bool),
        Literal::Char(_) => primitive_type_id(types, PrimitiveType::Char),
        Literal::String(_) => primitive_type_id(types, PrimitiveType::String),
        Literal::Unit => primitive_type_id(types, PrimitiveType::Unit),
    }
}
