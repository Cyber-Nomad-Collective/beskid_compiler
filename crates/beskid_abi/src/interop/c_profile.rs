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
use crate::interop::{CallShapeClass, InteropSignature, OwnershipClass, TypeShape};

/// The C ABI profile binding. Profiles are constructed once and validated
/// against the conformance envelope; the glue layer consults them when
/// mapping Beskid surface types to foreign signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CAbiProfile;

/// The scalar `AbiType`s the C profile permits at the user FFI boundary in
/// the current delivery band.
pub const C_PROFILE_PERMITTED_SCALARS: &[AbiType] =
    &[AbiType::I8, AbiType::U8, AbiType::I32, AbiType::I64, AbiType::F64];

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
    /// The C profile rejects this type-shape at the user FFI boundary.
    Rejected,
}

impl CAbiProfile {
    /// Bind a single `TypeShape` to its C profile view.
    pub fn bind(&self, shape: &TypeShape) -> CProfileBinding {
        match shape {
            TypeShape::Scalar(scalar) => {
                let permitted = C_PROFILE_PERMITTED_SCALARS.contains(&scalar.abi_type);
                CProfileBinding {
                    permitted,
                    view: if permitted { CProfileView::Scalar(scalar.abi_type) } else { CProfileView::Rejected },
                }
            }
            TypeShape::OpaqueHandle => CProfileBinding { permitted: true, view: CProfileView::OpaqueHandle },
            TypeShape::Buffer(buffer) if buffer.is_utf8 => {
                CProfileBinding { permitted: true, view: CProfileView::StringView }
            }
            TypeShape::Buffer(_) => CProfileBinding { permitted: true, view: CProfileView::Buffer },
            TypeShape::StringLike => CProfileBinding { permitted: true, view: CProfileView::StringView },
            TypeShape::Never => CProfileBinding { permitted: false, view: CProfileView::Rejected },
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
        }
    }
}

impl std::error::Error for CProfileError {}
