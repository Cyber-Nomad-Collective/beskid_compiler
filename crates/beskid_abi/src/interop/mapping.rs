//! Surface-type → `TypeShape` mapping for the `Interop.Contracts` vocabulary.
//!
//! `beskid_abi` cannot depend on `beskid_analysis`, so this module exposes a
//! local [`SurfacePrimitive`] mirror of the Beskid surface `PrimitiveType`
//! enum. Callers in `beskid_analysis` convert from
//! `beskid_analysis::syntax::PrimitiveType` to [`SurfacePrimitive`] before
//! consulting [`surface_primitive_to_type_shape`].
//!
//! Mapping rules (per the Interop.Contracts core-primitives article):
//! - `Bool`, `U8`, `I32`, `I64`, `F64` → `TypeShape::Scalar` bound to the
//!   matching `AbiType`.
//! - `Pointer` → `TypeShape::OpaqueHandle` (address-sized token).
//! - `Word` → `TypeShape::Scalar` bound to `AbiType::USize` (pointer-width
//!   unsigned).
//! - `Never` → `TypeShape::Never` (return-only; lowers to a trap/unwind).
//! - `String` → `None`. The surface `string` primitive is a GC reference that
//!   must not cross the FFI boundary directly; callers must use the
//!   `CStringView` interop view type instead. `TypeShape::StringLike` is the
//!   shape `CStringView` binds to, not the surface `string` primitive.
//! - `Char` → `None`. Not permitted on the FFI boundary.
//! - `Unit` → `None`. `Unit` is the void return marker, not a value type; the
//!   caller treats a `Unit` return as a void return slot (no validation).

use crate::abi_v5::AbiType;
use crate::interop::{ScalarShape, TypeShape};

/// Local mirror of `beskid_analysis::syntax::PrimitiveType` that
/// `beskid_abi` can consume without a dependency on `beskid_analysis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfacePrimitive {
    Bool,
    I32,
    I64,
    U8,
    Pointer,
    Word,
    F64,
    Char,
    String,
    Unit,
    Never,
}

impl SurfacePrimitive {
    /// Convert from the surface `PrimitiveType` name (lowercase keyword) to
    /// the local mirror. Returns `None` for unknown names.
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "bool" => Self::Bool,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "u8" => Self::U8,
            "pointer" => Self::Pointer,
            "word" => Self::Word,
            "f64" => Self::F64,
            "char" => Self::Char,
            "string" => Self::String,
            "unit" => Self::Unit,
            "never" => Self::Never,
            _ => return None,
        })
    }
}

/// Map a surface primitive to its FFI [`TypeShape`], or `None` when the type
/// is not permitted at the FFI boundary.
///
/// Returns `None` for `Char`, `String`, and `Unit`. A `Unit` return is the
/// void-return marker and is handled by the caller (no return slot). `Never`
/// returns `Some(TypeShape::Never)` and is valid as a return only.
pub fn surface_primitive_to_type_shape(primitive: SurfacePrimitive) -> Option<TypeShape> {
    match primitive {
        SurfacePrimitive::Bool => Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::I8 })),
        SurfacePrimitive::U8 => Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::U8 })),
        SurfacePrimitive::I32 => Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::I32 })),
        SurfacePrimitive::I64 => Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::I64 })),
        SurfacePrimitive::F64 => Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::F64 })),
        SurfacePrimitive::Pointer => Some(TypeShape::OpaqueHandle),
        SurfacePrimitive::Word => Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::USize })),
        SurfacePrimitive::Never => Some(TypeShape::Never),
        SurfacePrimitive::Char | SurfacePrimitive::String | SurfacePrimitive::Unit => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_map_to_scalar_shapes() {
        assert_eq!(
            surface_primitive_to_type_shape(SurfacePrimitive::Bool),
            Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::I8 }))
        );
        assert_eq!(
            surface_primitive_to_type_shape(SurfacePrimitive::I64),
            Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::I64 }))
        );
        assert_eq!(
            surface_primitive_to_type_shape(SurfacePrimitive::F64),
            Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::F64 }))
        );
    }

    #[test]
    fn pointer_maps_to_opaque_handle() {
        assert_eq!(surface_primitive_to_type_shape(SurfacePrimitive::Pointer), Some(TypeShape::OpaqueHandle));
    }

    #[test]
    fn word_maps_to_usize_scalar() {
        assert_eq!(
            surface_primitive_to_type_shape(SurfacePrimitive::Word),
            Some(TypeShape::Scalar(ScalarShape { abi_type: AbiType::USize }))
        );
    }

    #[test]
    fn never_maps_to_never_shape() {
        assert_eq!(surface_primitive_to_type_shape(SurfacePrimitive::Never), Some(TypeShape::Never));
    }

    #[test]
    fn char_string_unit_are_not_permitted() {
        assert_eq!(surface_primitive_to_type_shape(SurfacePrimitive::Char), None);
        assert_eq!(surface_primitive_to_type_shape(SurfacePrimitive::String), None);
        assert_eq!(surface_primitive_to_type_shape(SurfacePrimitive::Unit), None);
    }

    #[test]
    fn from_keyword_round_trips_known_primitives() {
        assert_eq!(SurfacePrimitive::from_keyword("bool"), Some(SurfacePrimitive::Bool));
        assert_eq!(SurfacePrimitive::from_keyword("never"), Some(SurfacePrimitive::Never));
        assert_eq!(SurfacePrimitive::from_keyword("unknown"), None);
    }
}
