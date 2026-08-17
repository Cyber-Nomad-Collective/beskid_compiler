//! Typed instantiation of the normative `Interop.Contracts` vocabulary.
//!
//! This module is the single source of truth for the language-agnostic
//! boundary vocabulary defined by the
//! `language-meta--interop--interop-contracts` capability. It declares the
//! type-shape classes, call-shape classes, ownership classes, and conformance
//! envelope as typed, validated, serde-serializable values. C and Rust ABI
//! profiles bind these primitives through the `c_profile` and `rust_profile`
//! submodules; they do not redefine them.
//!
//! `Beskid.Glue` consumes these primitives and adds glue-specific generation
//! and reading constructs on top. This module does not model glue emission,
//! signature reading, or toolchain probing; those live in the glue layer and
//! the `toolchain` module.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::abi_v5::AbiType;

/// A type-shape class, per the Interop.Contracts core-primitives article.
///
/// Every value that crosses a foreign boundary belongs to exactly one
/// type-shape class. Profiles bind each class to a concrete layout and
/// calling convention; the glue layer maps Beskid surface types to these
/// classes before emission or after reading a foreign signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeShapeClass {
    /// Fixed-width integers and enums. Bound to a single `AbiType` scalar.
    Scalar,
    /// Address-sized token with no Beskid layout on the foreign side.
    OpaqueHandle,
    /// Length-associated byte or element range. Ownership is tracked separately.
    Buffer,
    /// Bounded UTF-8 text. Aligns with the runtime `BeskidStr` header.
    StringLike,
    /// Divergence. Lowers to a trap or unwind; never returned as a value.
    Never,
}

/// A scalar type-shape, binding a `TypeShapeClass::Scalar` to a concrete
/// `AbiType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarShape {
    pub abi_type: AbiType,
}

/// A buffer or string-like type-shape. `element_type` is `AbiType::U8` for
/// byte buffers and `StringLike`; element buffers carry their own element
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BufferShape {
    pub element_type: AbiType,
    /// True when the foreign side treats the range as UTF-8 text.
    pub is_utf8: bool,
}

/// A concrete type-shape: a class plus the binding data the class requires.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "class", content = "binding", deny_unknown_fields)]
pub enum TypeShape {
    Scalar(ScalarShape),
    OpaqueHandle,
    Buffer(BufferShape),
    StringLike,
    Never,
}

impl TypeShape {
    pub fn class(&self) -> TypeShapeClass {
        match self {
            Self::Scalar(_) => TypeShapeClass::Scalar,
            Self::OpaqueHandle => TypeShapeClass::OpaqueHandle,
            Self::Buffer(_) => TypeShapeClass::Buffer,
            Self::StringLike => TypeShapeClass::StringLike,
            Self::Never => TypeShapeClass::Never,
        }
    }
}

/// An ownership class, per the Interop.Contracts ownership-at-boundary
/// article. Describes who is responsible for releasing a value after it
/// crosses the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipClass {
    /// The caller retains ownership; the callee must not release the value.
    Borrow,
    /// Ownership transfers to the callee, which must release it.
    Transfer,
    /// The value is borrowed but opaque: the callee may not inspect the
    /// layout, only hold and return the handle.
    OpaqueBorrow,
}

/// A call-shape class, per the Interop.Contracts call-shape article.
/// Describes how a single parameter or return value is passed across the
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallShapeClass {
    /// Passed by value in a register or stack slot per the platform ABI.
    Direct,
    /// Passed as a pointer to the value; the callee may not retain it beyond
    /// the call unless the ownership class is `Transfer`.
    ByReference,
    /// A length-associated view (`CStringView`, `CBuffer`, `CArrayView`)
    /// passed as a `(pointer, length[, capacity])` record.
    View,
}

/// One parameter of a boundary signature: its type-shape, call-shape, and
/// ownership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteropParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeShape,
    pub call: CallShapeClass,
    pub ownership: OwnershipClass,
}

/// The return slot of a boundary signature. `Never` is permitted; it lowers
/// to a trap or unwind and carries no value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteropReturn {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub ty: TypeShape,
    pub ownership: OwnershipClass,
}

/// A boundary signature: the typed, profile-agnostic description of one
/// foreign-callable or foreign-calling operation. Profiles and glue
/// backends bind this to a concrete symbol and calling convention.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteropSignature {
    pub symbol: String,
    pub parameters: Vec<InteropParameter>,
    pub returns: InteropReturn,
    /// True when a call to this signature never returns normally.
    pub no_return: bool,
}

/// The conformance envelope version band. User-FFI layout changes are
/// versioned independently of the runtime ABI through
/// `BESKID_USER_FFI_LAYOUT_BAND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceEnvelope {
    pub runtime_abi_version: u32,
    pub user_ffi_layout_band: u32,
}

impl ConformanceEnvelope {
    pub fn current() -> Self {
        Self {
            runtime_abi_version: crate::BESKID_RUNTIME_ABI_VERSION,
            user_ffi_layout_band: crate::generated::symbols::BESKID_USER_FFI_LAYOUT_BAND,
        }
    }

    pub fn validate(&self) -> Result<(), ConformanceEnvelopeError> {
        if self.runtime_abi_version != crate::BESKID_RUNTIME_ABI_VERSION {
            return Err(ConformanceEnvelopeError::RuntimeAbiVersionMismatch {
                expected: crate::BESKID_RUNTIME_ABI_VERSION,
                actual: self.runtime_abi_version,
            });
        }
        if self.user_ffi_layout_band != crate::generated::symbols::BESKID_USER_FFI_LAYOUT_BAND {
            return Err(ConformanceEnvelopeError::UserFfiLayoutBandMismatch {
                expected: crate::generated::symbols::BESKID_USER_FFI_LAYOUT_BAND,
                actual: self.user_ffi_layout_band,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceEnvelopeError {
    RuntimeAbiVersionMismatch { expected: u32, actual: u32 },
    UserFfiLayoutBandMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for ConformanceEnvelopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RuntimeAbiVersionMismatch { expected, actual } => {
                write!(f, "runtime ABI version mismatch: expected {expected}, actual {actual}")
            }
            Self::UserFfiLayoutBandMismatch { expected, actual } => {
                write!(f, "user FFI layout band mismatch: expected {expected}, actual {actual}")
            }
        }
    }
}

impl std::error::Error for ConformanceEnvelopeError {}

impl InteropSignature {
    /// Validate the signature against the Interop.Contracts invariants.
    ///
    /// This enforces the structural rules the vocabulary requires: no
    /// duplicate parameter names, a `Never` return implies `no_return`, and
    /// every type-shape is well-formed. Profile-specific binding rules
    /// (permitted scalar widths, view layouts) live in the profile modules.
    pub fn validate(&self) -> Result<(), InteropSignatureError> {
        let mut names = HashSet::new();
        for parameter in &self.parameters {
            if parameter.name.is_empty() {
                return Err(InteropSignatureError::EmptyParameterName);
            }
            if !names.insert(parameter.name.clone()) {
                return Err(InteropSignatureError::DuplicateParameter(parameter.name.clone()));
            }
            validate_type_shape(&parameter.ty)?;
        }
        validate_type_shape(&self.returns.ty)?;
        if matches!(self.returns.ty, TypeShape::Never) && !self.no_return {
            return Err(InteropSignatureError::NeverReturnWithoutNoReturn);
        }
        Ok(())
    }
}

fn validate_type_shape(shape: &TypeShape) -> Result<(), InteropSignatureError> {
    match shape {
        TypeShape::Scalar(scalar) => {
            if matches!(scalar.abi_type, AbiType::Void) {
                return Err(InteropSignatureError::VoidScalar);
            }
            if matches!(scalar.abi_type, AbiType::V128) {
                return Err(InteropSignatureError::UnsupportedScalarWidth(scalar.abi_type));
            }
        }
        TypeShape::Buffer(buffer) => {
            if matches!(buffer.element_type, AbiType::Void) {
                return Err(InteropSignatureError::VoidBufferElement);
            }
        }
        TypeShape::OpaqueHandle | TypeShape::StringLike | TypeShape::Never => {}
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteropSignatureError {
    EmptyParameterName,
    DuplicateParameter(String),
    NeverReturnWithoutNoReturn,
    VoidScalar,
    VoidBufferElement,
    UnsupportedScalarWidth(AbiType),
}

impl std::fmt::Display for InteropSignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyParameterName => write!(f, "interop signature has an empty parameter name"),
            Self::DuplicateParameter(name) => {
                write!(f, "interop signature has a duplicate parameter `{name}`")
            }
            Self::NeverReturnWithoutNoReturn => {
                write!(f, "interop signature returns `Never` but is not marked `no_return`")
            }
            Self::VoidScalar => write!(f, "scalar type-shape cannot bind `Void`"),
            Self::VoidBufferElement => write!(f, "buffer type-shape cannot have a `Void` element"),
            Self::UnsupportedScalarWidth(ty) => {
                write!(f, "scalar type-shape binds an unsupported width `{ty:?}`")
            }
        }
    }
}

impl std::error::Error for InteropSignatureError {}

pub mod c_profile;
pub mod mapping;
pub mod rust_profile;
