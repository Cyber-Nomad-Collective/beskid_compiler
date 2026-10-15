//! C ABI profile binding of the `Interop.Contracts` primitives.
//!
//! The C ABI profile binds symbols, layouts, linking, and unwind rules to
//! the `Interop.Contracts` vocabulary. This module defines which
//! `TypeShape`/`CallShapeClass`/`OwnershipClass` combinations the C profile
//! permits at the user FFI boundary in the current delivery band, and how
//! each maps to a C ABI view record.
//!
//! Per the C ABI profile, Beskid `string` and `T[]` MUST NOT cross the user
//! FFI boundary as ordinary GC references; they cross as interop view
//! types (`CStringView`, `CBuffer`, `CArrayView`).

use crate::abi_v5::AbiType;
use crate::interop::{
    CallShapeClass, InteropSignature, OwnershipClass, RecordFieldShape, RecordShape, ScalarDomain, ScalarShape, TypeShape,
};

/// Method an `[Extern(..., Optional:true)]` contract reserves for its availability query. In an
/// optional contract, `bool Available();` is supplied by the compiler and is not a C symbol: it
/// returns true only when every other symbol of the contract resolved.
pub const OPTIONAL_EXTERN_AVAILABILITY_METHOD: &str = "Available";

/// ABI-v5 trap name raised when a program calls a symbol of an optional `[Extern]` contract that
/// did not resolve (its library or the symbol is absent at load time).
pub const OPTIONAL_EXTERN_UNAVAILABLE_TRAP: &str = "extern_unavailable";

/// The C ABI profile binding. Profiles are constructed once and validated
/// against the conformance envelope; the glue layer consults them when
/// mapping Beskid surface types to foreign signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CAbiProfile;

/// The scalar `AbiType`s the C profile permits at the user FFI boundary in
/// the current delivery band.
pub const C_PROFILE_PERMITTED_SCALARS: &[AbiType] = &[
    AbiType::I8,
    AbiType::I16,
    AbiType::U16,
    AbiType::U32,
    AbiType::U64,
    AbiType::F32,
    AbiType::U8,
    AbiType::I32,
    AbiType::I64,
    AbiType::F64,
];

/// A C profile binding decision for a single type-shape: whether it is
/// permitted, and if so how it is passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CProfileBinding {
    pub permitted: bool,
    pub view: CProfileView,
}

/// The C profile view record a type-shape lowers to, when permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CProfileView {
    /// Passed by value as the scalar `AbiType`.
    Scalar(AbiType),
    /// An opaque handle passed as a pointer-width integer.
    OpaqueHandle,
    /// `CBuffer { ptr, len }` view.
    Buffer,
    /// `CStringView { ptr, len }` view aligning with `BeskidStr`.
    StringView,
    /// A checked `uint64_t` handle token.
    HandleToken,
    /// A C struct passed by value with the profile layout.
    Record { size: u64, alignment: u64 },
    /// A `uint32_t` discriminant restricted to the variant count.
    Discriminant { variants: u32 },
    /// `int32_t (*)(uint64_t, ...)` plus a `uint64_t` context token.
    CallbackPointer,
    /// The `void` return of a unit result.
    Void,
    /// The C profile rejects this type-shape at the user FFI boundary.
    Rejected,
}

/// Maximum number of fields in a by-value record.
pub const C_PROFILE_RECORD_FIELD_LIMIT: usize = 64;
/// Maximum number of variants in a fieldless enumeration.
pub const C_PROFILE_ENUMERATION_VARIANT_LIMIT: usize = 4096;
/// Maximum number of callback parameters.
pub const C_PROFILE_CALLBACK_PARAMETER_LIMIT: usize = 16;

/// The C struct layout of a record: field offsets in declaration order, alignment and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CRecordLayout {
    pub offsets: Vec<u64>,
    pub size: u64,
    pub alignment: u64,
}

/// Size and natural alignment of a C scalar on the supported 64-bit targets
/// (x86_64 System V, aarch64 AAPCS64/Apple, x86_64 Microsoft).
pub fn c_scalar_layout(abi_type: AbiType) -> Option<(u64, u64)> {
    Some(match abi_type {
        AbiType::I8 | AbiType::U8 => (1, 1),
        AbiType::I16 | AbiType::U16 => (2, 2),
        AbiType::I32 | AbiType::U32 | AbiType::F32 => (4, 4),
        AbiType::I64 | AbiType::U64 | AbiType::F64 | AbiType::USize | AbiType::ISize | AbiType::Pointer => (8, 8),
        AbiType::Void | AbiType::V128 => return None,
    })
}

/// Whether a scalar is admitted at the C boundary: the permitted fixed-width scalars, plus the
/// target word in the native-word domain. The Bool and Unicode scalar domains bind only
/// `uint8_t` and `uint32_t`.
fn scalar_permitted(scalar: &ScalarShape) -> bool {
    match scalar.domain {
        ScalarDomain::Raw => C_PROFILE_PERMITTED_SCALARS.contains(&scalar.abi_type),
        ScalarDomain::Bool => scalar.abi_type == AbiType::U8,
        ScalarDomain::UnicodeScalar => scalar.abi_type == AbiType::U32,
        ScalarDomain::NativeWord => scalar.abi_type == AbiType::USize,
    }
}

impl CAbiProfile {
    /// Bind a single `TypeShape` to its C profile view.
    /// Compute the C struct layout of `fields` in declaration order: each field at the next
    /// offset aligned to its natural alignment, the record aligned to its largest field, and the
    /// size rounded up to that alignment. This is the aggregate rule of the System V AMD64
    /// psABI, AAPCS64 and the Microsoft x64 ABI, and the Rust `#[repr(C)]` algorithm.
    pub fn record_layout(&self, fields: &[ScalarShape]) -> Result<CRecordLayout, CProfileError> {
        if fields.is_empty() || fields.len() > C_PROFILE_RECORD_FIELD_LIMIT {
            return Err(CProfileError::RecordFieldBound);
        }
        let mut offsets = Vec::with_capacity(fields.len());
        let mut end = 0u64;
        let mut alignment = 1u64;
        for field in fields {
            if !scalar_permitted(field) {
                return Err(CProfileError::RecordField);
            }
            let (size, align) = c_scalar_layout(field.abi_type).ok_or(CProfileError::RecordField)?;
            end = end.checked_add(align - 1).ok_or(CProfileError::RecordFieldBound)? & !(align - 1);
            offsets.push(end);
            end = end.checked_add(size).ok_or(CProfileError::RecordFieldBound)?;
            alignment = alignment.max(align);
        }
        let size = end.checked_add(alignment - 1).ok_or(CProfileError::RecordFieldBound)? & !(alignment - 1);
        Ok(CRecordLayout { offsets, size, alignment })
    }

    /// Form the record shape of named scalar fields with the profile layout.
    pub fn record_shape(&self, identity: &str, fields: &[(String, ScalarShape)]) -> Result<RecordShape, CProfileError> {
        let layout = self.record_layout(&fields.iter().map(|(_, scalar)| *scalar).collect::<Vec<_>>())?;
        Ok(RecordShape {
            identity: identity.to_owned(),
            fields: fields
                .iter()
                .zip(&layout.offsets)
                .map(|((name, scalar), offset)| RecordFieldShape { name: name.clone(), scalar: *scalar, offset: *offset })
                .collect(),
            size: layout.size,
            alignment: layout.alignment,
        })
    }

    /// Bind a single `TypeShape` to its C profile view.
    pub fn bind(&self, shape: &TypeShape) -> CProfileBinding {
        let rejected = CProfileBinding { permitted: false, view: CProfileView::Rejected };
        match shape {
            TypeShape::Scalar(scalar) => {
                let permitted = scalar_permitted(scalar);
                CProfileBinding {
                    permitted,
                    view: if permitted { CProfileView::Scalar(scalar.abi_type) } else { CProfileView::Rejected },
                }
            }
            TypeShape::BrandedHandle(handle) => {
                if handle.identity.is_empty() || handle.arguments.iter().any(|argument| !self.bind(argument).permitted) {
                    return rejected;
                }
                CProfileBinding { permitted: true, view: CProfileView::HandleToken }
            }
            TypeShape::Record(record) => {
                let scalars = record.fields.iter().map(|field| field.scalar).collect::<Vec<_>>();
                match self.record_layout(&scalars) {
                    Ok(layout)
                        if layout.size == record.size
                            && layout.alignment == record.alignment
                            && layout.offsets.iter().eq(record.fields.iter().map(|field| &field.offset)) =>
                    {
                        CProfileBinding {
                            permitted: true,
                            view: CProfileView::Record { size: layout.size, alignment: layout.alignment },
                        }
                    }
                    _ => rejected,
                }
            }
            TypeShape::Enumeration(enumeration) => {
                if enumeration.variants.is_empty() || enumeration.variants.len() > C_PROFILE_ENUMERATION_VARIANT_LIMIT {
                    return rejected;
                }
                CProfileBinding {
                    permitted: true,
                    view: CProfileView::Discriminant { variants: enumeration.variants.len() as u32 },
                }
            }
            TypeShape::Callback(callback) => {
                let scalar = |shape: &TypeShape| {
                    matches!(shape, TypeShape::Scalar(_) | TypeShape::Enumeration(_)) && self.bind(shape).permitted
                };
                if callback.parameters.len() > C_PROFILE_CALLBACK_PARAMETER_LIMIT
                    || !callback.parameters.iter().all(scalar)
                    || !callback.result.as_deref().is_none_or(scalar)
                {
                    return rejected;
                }
                CProfileBinding { permitted: true, view: CProfileView::CallbackPointer }
            }
            TypeShape::OpaqueHandle => CProfileBinding { permitted: true, view: CProfileView::OpaqueHandle },
            TypeShape::Buffer(buffer) if buffer.is_utf8 => {
                CProfileBinding { permitted: true, view: CProfileView::StringView }
            }
            TypeShape::Buffer(_) => CProfileBinding { permitted: true, view: CProfileView::Buffer },
            TypeShape::StringLike => CProfileBinding { permitted: true, view: CProfileView::StringView },
            TypeShape::Never => CProfileBinding { permitted: false, view: CProfileView::Rejected },
            TypeShape::Unit => CProfileBinding { permitted: true, view: CProfileView::Void },
        }
    }

    /// Validate a full signature against the C profile. A signature is
    /// C-profile-conformant when every parameter and the return shape are
    /// permitted, and the return shape is `Never` only when `no_return` is
    /// set.
    pub fn validate_signature(&self, signature: &InteropSignature) -> Result<(), CProfileError> {
        for parameter in &signature.parameters {
            let binding = self.bind(&parameter.ty);
            if !binding.permitted {
                return Err(CProfileError::DisallowedShape { parameter: parameter.name.clone() });
            }
            if matches!(parameter.ownership, OwnershipClass::Transfer)
                && !matches!(parameter.call, CallShapeClass::Direct | CallShapeClass::View)
            {
                return Err(CProfileError::TransferRequiresDirectOrView { parameter: parameter.name.clone() });
            }
        }
        let return_binding = self.bind(&signature.returns.ty);
        if !return_binding.permitted && !matches!(signature.returns.ty, TypeShape::Never) {
            return Err(CProfileError::DisallowedReturn);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CProfileError {
    DisallowedShape { parameter: String },
    TransferRequiresDirectOrView { parameter: String },
    DisallowedReturn,
    RecordField,
    RecordFieldBound,
}

impl std::fmt::Display for CProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DisallowedShape { parameter } => {
                write!(f, "C profile disallows the type-shape of parameter `{parameter}`")
            }
            Self::TransferRequiresDirectOrView { parameter } => write!(
                f,
                "C profile requires `Transfer` ownership to use `Direct` or `View` call-shape for parameter `{parameter}`"
            ),
            Self::DisallowedReturn => write!(f, "C profile disallows the return type-shape"),
            Self::RecordField => write!(f, "C profile record fields must be permitted scalars"),
            Self::RecordFieldBound => write!(f, "C profile records have 1..=64 fields"),
        }
    }
}

impl std::error::Error for CProfileError {}

#[cfg(test)]
mod record_tests {
    use super::*;
    use crate::interop::{CallbackShape, EnumerationShape};

    fn raw(abi_type: AbiType) -> ScalarShape {
        ScalarShape::raw(abi_type)
    }

    #[test]
    fn record_layout_matches_the_c_struct_rule() {
        let layout = CAbiProfile.record_layout(&[raw(AbiType::U8), raw(AbiType::I64)]).unwrap();
        assert_eq!(layout, CRecordLayout { offsets: vec![0, 8], size: 16, alignment: 8 });
        let layout =
            CAbiProfile.record_layout(&[raw(AbiType::I64), raw(AbiType::I64), raw(AbiType::F64)]).unwrap();
        assert_eq!(layout, CRecordLayout { offsets: vec![0, 8, 16], size: 24, alignment: 8 });
        let layout = CAbiProfile
            .record_layout(&[
                raw(AbiType::U16),
                ScalarShape::new(AbiType::U8, ScalarDomain::Bool),
                ScalarShape::new(AbiType::U32, ScalarDomain::UnicodeScalar),
                raw(AbiType::U8),
            ])
            .unwrap();
        assert_eq!(layout, CRecordLayout { offsets: vec![0, 2, 4, 8], size: 12, alignment: 4 });
        let layout = CAbiProfile.record_layout(&[raw(AbiType::F32), raw(AbiType::I8)]).unwrap();
        assert_eq!(layout, CRecordLayout { offsets: vec![0, 4], size: 8, alignment: 4 });
    }

    #[test]
    fn record_shapes_bind_only_with_the_profile_layout() {
        let fields = vec![("tag".to_owned(), raw(AbiType::U8)), ("value".to_owned(), raw(AbiType::I64))];
        let mut shape = CAbiProfile.record_shape("Main.Pair", &fields).unwrap();
        assert_eq!(
            CAbiProfile.bind(&TypeShape::Record(shape.clone())),
            CProfileBinding { permitted: true, view: CProfileView::Record { size: 16, alignment: 8 } }
        );
        shape.fields[1].offset = 4;
        assert!(!CAbiProfile.bind(&TypeShape::Record(shape)).permitted);
        assert!(CAbiProfile.record_layout(&[]).is_err());
        assert!(CAbiProfile.record_layout(&vec![raw(AbiType::U8); 65]).is_err());
        assert!(CAbiProfile.record_layout(&[raw(AbiType::Pointer)]).is_err());
        assert!(CAbiProfile.record_layout(&[ScalarShape::new(AbiType::U32, ScalarDomain::Bool)]).is_err());
    }

    #[test]
    fn bool_and_u8_are_distinct_shapes_with_one_c_type() {
        let boolean = TypeShape::Scalar(ScalarShape::new(AbiType::U8, ScalarDomain::Bool));
        let byte = TypeShape::Scalar(raw(AbiType::U8));
        assert_ne!(boolean, byte);
        assert_eq!(CAbiProfile.bind(&boolean).view, CAbiProfile.bind(&byte).view);
        let word = TypeShape::Scalar(ScalarShape::new(AbiType::USize, ScalarDomain::NativeWord));
        assert!(CAbiProfile.bind(&word).permitted);
        assert!(!CAbiProfile.bind(&TypeShape::Scalar(raw(AbiType::USize))).permitted);
    }

    #[test]
    fn enumerations_and_callbacks_bind_within_their_bounds() {
        let enumeration = TypeShape::Enumeration(EnumerationShape {
            identity: "Main.Color".into(),
            variants: vec!["Red".into(), "Green".into()],
        });
        assert_eq!(CAbiProfile.bind(&enumeration).view, CProfileView::Discriminant { variants: 2 });
        let callback = CallbackShape { parameters: vec![TypeShape::Scalar(raw(AbiType::I64)), enumeration], result: None };
        assert_eq!(CAbiProfile.bind(&TypeShape::Callback(callback.clone())).view, CProfileView::CallbackPointer);
        let mut nested = callback;
        nested.parameters.push(TypeShape::StringLike);
        assert!(!CAbiProfile.bind(&TypeShape::Callback(nested)).permitted);
    }
}
