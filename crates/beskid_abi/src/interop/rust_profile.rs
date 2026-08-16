//! Rust ABI profile binding of the `Interop.Contracts` primitives.
//!
//! The Rust ABI profile describes the Rust-hosted runtime surface: exported
//! symbols, unwind at the boundary, and stability rules distinct from user C
//! extern libraries. It is NOT a promise that arbitrary Rust crates can be
//! user `Extern` targets without shims; user foreign code stays on the C ABI
//! profile.
//!
//! This profile binds the `Interop.Contracts` vocabulary to the runtime
//! symbol surface (`beskid_rt_v5_*` and library lifecycle symbols) rather
//! than to user foreign libraries.

use crate::abi_v5::{AbiType, LIBRARY_LIFECYCLE_SYMBOLS, RUNTIME_SYMBOL_PREFIX};
use crate::interop::{InteropSignature, TypeShape};

/// The Rust ABI profile binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustAbiProfile;

/// The scalar `AbiType`s the Rust runtime profile exposes on the kernel
/// surface.
pub const RUST_PROFILE_PERMITTED_SCALARS: &[AbiType] = &[
    AbiType::I8,
    AbiType::U8,
    AbiType::I32,
    AbiType::I64,
    AbiType::F64,
    AbiType::USize,
    AbiType::ISize,
    AbiType::Pointer,
];

impl RustAbiProfile {
    /// Bind a single `TypeShape` to a Rust-runtime scalar or view. The
    /// Rust runtime exposes the full managed-object and closure-environment
    /// surface, so `OpaqueHandle` maps to `Pointer` and buffers map to
    /// pointer/length pairs handled by the runtime.
    pub fn bind(&self, shape: &TypeShape) -> RustProfileBinding {
        match shape {
            TypeShape::Scalar(scalar) => {
                let permitted = RUST_PROFILE_PERMITTED_SCALARS.contains(&scalar.abi_type);
                RustProfileBinding {
                    permitted,
                    view: if permitted { RustProfileView::Scalar(scalar.abi_type) } else { RustProfileView::Rejected },
                }
            }
            TypeShape::OpaqueHandle => RustProfileBinding { permitted: true, view: RustProfileView::Pointer },
            TypeShape::Buffer(_) | TypeShape::StringLike => {
                RustProfileBinding { permitted: true, view: RustProfileView::ManagedView }
            }
            TypeShape::Never => RustProfileBinding { permitted: true, view: RustProfileView::Trap },
        }
    }

    /// Validate that a signature names a runtime-owned symbol. The Rust
    /// profile requires every exported symbol to be versioned with the
    /// `beskid_rt_v5_` prefix or be one of the library lifecycle symbols.
    pub fn validate_runtime_symbol(&self, signature: &InteropSignature) -> Result<(), RustProfileError> {
        if !signature.symbol.starts_with(RUNTIME_SYMBOL_PREFIX)
            && !LIBRARY_LIFECYCLE_SYMBOLS.contains(&signature.symbol.as_str())
        {
            return Err(RustProfileError::UnversionedSymbol { symbol: signature.symbol.clone() });
        }
        for parameter in &signature.parameters {
            let binding = self.bind(&parameter.ty);
            if !binding.permitted {
                return Err(RustProfileError::DisallowedShape { parameter: parameter.name.clone() });
            }
        }
        let return_binding = self.bind(&signature.returns.ty);
        if !return_binding.permitted {
            return Err(RustProfileError::DisallowedReturn);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustProfileBinding {
    pub permitted: bool,
    pub view: RustProfileView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustProfileView {
    Scalar(AbiType),
    Pointer,
    ManagedView,
    Trap,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustProfileError {
    UnversionedSymbol { symbol: String },
    DisallowedShape { parameter: String },
    DisallowedReturn,
}

impl std::fmt::Display for RustProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnversionedSymbol { symbol } => {
                write!(
                    f,
                    "Rust profile requires a `beskid_rt_v5_`-prefixed or library lifecycle symbol, got `{symbol}`"
                )
            }
            Self::DisallowedShape { parameter } => {
                write!(f, "Rust profile disallows the type-shape of parameter `{parameter}`")
            }
            Self::DisallowedReturn => write!(f, "Rust profile disallows the return type-shape"),
        }
    }
}

impl std::error::Error for RustProfileError {}
