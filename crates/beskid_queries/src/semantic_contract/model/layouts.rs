//! Aggregate and enum layout, constructor, and match facts.

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

/// Target-neutral storage shape for one source aggregate field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AggregateFieldShape {
    Scalar(SemanticTypeId),
    Nominal(AstNodeKey),
}

/// Source-ordered, named fields of one nominal `type` definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AggregateLayoutFact {
    pub fields: Arc<[(Arc<str>, AggregateFieldShape)]>,
}

/// Source names paired with the current-generation value expressions of one aggregate literal.
pub type AggregateLiteralFieldValues = Arc<[(Arc<str>, AstNodeKey)]>;

/// Exact nominal field selected by a local, nominal field chain, or implicit method receiver.
///
/// The receiver must resolve through the current syntax generation to a parameter, an explicitly
/// typed local, a real path-segment projection, the enclosing nominal method, or a generic call result whose complete
/// specialization proves one nominal return layout. More dynamic member shapes intentionally
/// remain unavailable until they have their own syntax authority.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AggregateFieldAccess {
    pub declaration: AstNodeKey,
    pub receiver: AstNodeKey,
    pub index: u32,
    /// Exact applied field layout of the receiver. Generic aggregate arguments are
    /// materialized here rather than being reconstructed by code generation.
    pub layout: AggregateLayoutFact,
}

/// Target-specific ABI layout of one semantic scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ScalarAbiLayout {
    pub size: u64,
    pub alignment: u64,
    pub is_pointer: bool,
}

/// Exact ABI-v5 storage selected by one source enum variant, in source field order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumScalarPayloadVariantLayout {
    /// Unit fields retain their source position as `None` while consuming no physical storage.
    pub payload_fields: Arc<[Option<(SemanticTypeId, u64)>]>,
}

/// Target-specific managed-object layout for a source enum payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumScalarPayloadObjectLayout {
    pub object_size: u64,
    pub object_alignment: u64,
    pub tag_offset: u64,
    pub storage_fields: Arc<[(SemanticTypeId, u64)]>,
    pub pointer_map_offsets: Arc<[u64]>,
    pub variants: Arc<[EnumScalarPayloadVariantLayout]>,
}

/// Source-ordered variants and fields of one nominal `enum` definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumLayoutFact {
    pub variants: Arc<[EnumVariantLayoutFact]>,
}

/// One source enum variant with its source-ordered named fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumVariantLayoutFact {
    pub name: Arc<str>,
    pub fields: Arc<[(Arc<str>, AggregateFieldShape)]>,
}

/// Exact enum declaration, source-order variant, and ordered payloads selected by a constructor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumConstructorFact {
    pub declaration: AstNodeKey,
    pub variant_index: u32,
    pub payloads: Arc<[AstNodeKey]>,
}

/// One contextual generic enum argument retained until its enclosing item is specialized.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnumLayoutTemplateArgument {
    Concrete(AggregateFieldShape),
    EnclosingParameter(Arc<str>),
}

/// Source-owned generic enum constructor whose concrete layout depends on an enclosing item.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumConstructorTemplate {
    pub constructor: EnumConstructorFact,
    pub parameters: Arc<[Arc<str>]>,
    pub arguments: Arc<[EnumLayoutTemplateArgument]>,
}

/// Concrete constructor and enum layout derived from one immutable item specialization.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumConstructorSpecialization {
    pub constructor: EnumConstructorFact,
    pub layout: EnumLayoutFact,
}

/// One identifier binding within a recursively matched enum payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchBindingFact {
    /// Exact identifier declaration introduced by the match pattern.
    pub declaration: AstNodeKey,
    /// Source-proven ABI shape of the single matched variant payload.
    pub payload: AggregateFieldShape,
    /// Source-proven ownership class retained independently of pointer-shaped ABI storage.
    pub managed_reference: ManagedReferenceKind,
    /// Exact applied payload identity, when source syntax proves it. Never recovered from ABI.
    pub(in crate::semantic_contract) source_identity: Option<GenericSourceTypeIdentity>,
}

/// One scalar literal comparison in a recursive match pattern.
///
/// The source value, its explicit semantic type, and its current-generation syntax identity are
/// retained together so lowering does not need to infer comparison semantics from raw text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchScalarLiteralFact {
    pub literal: AstNodeKey,
    pub semantic_type: SemanticTypeId,
    pub value: LiteralFact,
}

/// One enum variant selected inside a recursive match pattern.
///
/// Every nested nominal enum carries its exact applied source layout. Target-specific offsets are
/// deliberately absent and remain the responsibility of ABI lowering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchVariantPatternFact {
    pub declaration: AstNodeKey,
    pub layout: EnumLayoutFact,
    pub variant_index: u32,
    pub items: Arc<[EnumMatchPatternFact]>,
}

/// Target-neutral recursive pattern authority for one source match arm.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnumMatchPatternFact {
    Wildcard,
    Binding(EnumMatchBindingFact),
    UnitLiteral { literal: AstNodeKey },
    ScalarLiteral(EnumMatchScalarLiteralFact),
    Enum(EnumMatchVariantPatternFact),
}

/// One source arm consumed by enum-match lowering.
///
/// Guards remain unavailable. Pattern structure is expressed once through the recursive tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchArmFact {
    pub pattern: EnumMatchPatternFact,
    pub body: AstNodeKey,
}

/// Exact enum declaration and source-ordered arms selected by a `match` expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EnumMatchFact {
    pub declaration: AstNodeKey,
    /// Concrete layout selected by an explicitly typed local or parameter scrutinee.
    ///
    /// Keeping this applied source layout on the match fact lets codegen lower generic enums
    /// without reconstructing type arguments from retired HIR artifacts.
    pub layout: EnumLayoutFact,
    pub arms: Arc<[EnumMatchArmFact]>,
}
