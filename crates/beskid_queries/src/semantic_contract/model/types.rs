//! Semantic type identities, collection operations, call lowering, and manifest builtins.

use beskid_abi::{abi_v5::AbiType, runtime_source::RuntimeIntrinsicCapability};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::inputs::ProjectSession;

use super::super::queries::{node_kind, node_span};
use super::*;

/// Opaque semantic type identity owned by the query layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SemanticTypeId(pub u32);

/// Whether a source value is traced by the Beskid garbage collector.
///
/// `NativeOrScalar` intentionally includes the source `pointer` primitive: sharing a machine
/// representation with managed values never makes an opaque native address a GC root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ManagedReferenceKind {
    GcManaged,
    NativeOrScalar,
}

impl SemanticTypeId {
    pub const UNIT: Self = Self(0);
    pub const BOOL: Self = Self(1);
    pub const I32: Self = Self(2);
    pub const I64: Self = Self(3);
    pub const U8: Self = Self(4);
    pub const F64: Self = Self(5);
    pub const CHAR: Self = Self(6);
    pub const STRING: Self = Self(7);
    /// Pointer-width unsigned integer in Beskid source, represented as ABI `usize`.
    pub const WORD: Self = Self(8);
    /// Opaque native address used only by the canonical runtime source surface.
    pub const POINTER: Self = Self(9);
    /// Bottom type for operations which cannot return normally.
    pub const NEVER: Self = Self(10);
    /// Fixed-width unsigned 32-bit integer. Its CLIF storage is `i32`, but its semantics are unsigned.
    pub const U32: Self = Self(11);

    /// Return this semantic scalar's target-specific ABI size, alignment, and pointer-map class.
    pub fn scalar_abi_layout(self, pointer_width: u8) -> Option<ScalarAbiLayout> {
        let pointer_size = match pointer_width {
            32 => 4,
            64 => 8,
            _ => return None,
        };
        match self {
            Self::BOOL | Self::U8 => Some(ScalarAbiLayout { size: 1, alignment: 1, is_pointer: false }),
            Self::I32 | Self::U32 | Self::CHAR => Some(ScalarAbiLayout { size: 4, alignment: 4, is_pointer: false }),
            Self::I64 | Self::F64 => Some(ScalarAbiLayout { size: 8, alignment: 8, is_pointer: false }),
            Self::WORD => Some(ScalarAbiLayout { size: pointer_size, alignment: pointer_size, is_pointer: false }),
            Self::POINTER | Self::STRING => {
                Some(ScalarAbiLayout { size: pointer_size, alignment: pointer_size, is_pointer: true })
            }
            _ => None,
        }
    }

    /// Source-facing type name used in diagnostics and compiler traces.
    ///
    /// Matches the Beskid surface spellings (`string`, `i32`, `unit`, …). Unknown identities
    /// render as `type#N` so traces never invent a fake primitive name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UNIT => "unit",
            Self::BOOL => "bool",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U32 => "u32",
            Self::U8 => "u8",
            Self::F64 => "f64",
            Self::CHAR => "char",
            Self::STRING => "string",
            Self::WORD => "word",
            Self::POINTER => "pointer",
            Self::NEVER => "never",
            _ => "type#?",
        }
    }

    /// Format one identity for traces, including a stable `type#N` fallback for unknowns.
    pub fn display_name(self) -> String {
        match self.as_str() {
            "type#?" => format!("type#{}", self.0),
            name => name.to_owned(),
        }
    }
}

impl std::fmt::Display for SemanticTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display_name())
    }
}

/// Mutable storage that owns a replacement produced by canonical array growth.
///
/// Both forms are generation-bound and name storage that codegen can update before releasing the
/// construction root. Unsupported expressions deliberately have no owner fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CollectionMutationOwner {
    Local(LocalSlot),
    AggregateField { receiver: LocalSlot, declaration: AstNodeKey, index: u32 },
}

/// Compiler-owned operation selected only from the resolved canonical Core.Collections.Array declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CollectionOperation {
    Append { owner: CollectionMutationOwner },
    Capacity,
    Clear,
    RemoveLast,
}

/// Compiler-owned generic managed-array allocation encoded by canonical Foundation syntax.
///
/// The element remains a named generic parameter until module emission supplies the concrete
/// specialization environment. No element size or legacy runtime symbol crosses this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TypedArrayAllocation {
    pub element_parameter: Arc<str>,
    pub length: u64,
}

/// Backend-relevant call classification, detached from legacy HIR nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CallLowering {
    Direct(AstNodeKey),
    Dynamic,
    ManifestBuiltin(ManifestBuiltin),
    Runtime(RuntimeIntrinsic),
    CorelibService(CorelibService),
}

/// Source-callable runtime operation declared explicitly by the ABI-v5 manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ManifestBuiltin {
    pub name: &'static str,
    pub symbol: &'static str,
}

impl ManifestBuiltin {
    pub fn for_name(name: &str) -> Option<Self> {
        beskid_abi::generated::abi_v5_contract::ABI_V5_SOURCE_BUILTINS
            .iter()
            .find(|builtin| builtin.name == name)
            .map(|builtin| Self { name: builtin.name, symbol: builtin.symbol })
    }
}

impl serde::Serialize for ManifestBuiltin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name)
    }
}

impl<'de> serde::Deserialize<'de> for ManifestBuiltin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::for_name(&name).ok_or_else(|| serde::de::Error::custom(format!("unknown ManifestBuiltin `{name}`")))
    }
}
