//! Read-only, issuer-checked semantic shape boundary for compiler Mods.
use crate::syntax::{AstNodeId, PrimitiveType, SpanInfo, SyntaxGenerationId};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub struct ModSemanticHandle(u64);
impl ModSemanticHandle {
    /// Transport an opaque issuer token; only the issuing authority can validate it.
    pub fn from_token(token: u64) -> Self {
        Self(token)
    }
    pub fn token(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ModSemanticFieldType {
    Scalar(PrimitiveType),
    Nominal(ModSemanticHandle),
    Array(Box<ModSemanticFieldType>),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ModSemanticOwnership {
    NativeOrScalar,
    GcManaged,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticDeclaration {
    pub source_unit: PathBuf,
    pub generation: SyntaxGenerationId,
    pub node: AstNodeId,
    pub span: SpanInfo,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticField {
    pub name: String,
    pub declaration: ModSemanticDeclaration,
    pub ty: ModSemanticFieldType,
    pub ownership: ModSemanticOwnership,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticShape {
    pub name: String,
    /// Assembly resolution identity; not a portable serialization identity.
    pub declaration_identity: String,
    pub package_declaration: Option<ModPackageDeclaration>,
    pub declaration: ModSemanticDeclaration,
    pub type_arguments: Vec<ModSemanticFieldType>,
    pub body: ModSemanticShapeBody,
    /// None means structurally inspectable but ineligible for stable serialization identity.
    pub package: Option<crate::projects::VerifiedPackageIdentity>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModPackageDeclaration {
    pub source_path: String,
    pub lexical_path: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ModSemanticShapeBody {
    Record { fields: Vec<ModSemanticField> },
    Enum { variants: Vec<ModSemanticVariant> },
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticVariant {
    pub name: String,
    pub ordinal: u32,
    pub declaration: ModSemanticDeclaration,
    pub fields: Vec<ModSemanticField>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticError(pub String);
impl std::fmt::Display for ModSemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ModSemanticError {}
/// Caller-visible syntax route to a compiler-registered canonical declaration.
/// This descriptive DTO is never an ownership or semantic witness.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModCanonicalPath {
    pub logical_name: String,
    pub segments: Vec<String>,
}

/// Issued signature of a caller-visible source function. Parameter and result handles are
/// the same issuer tokens `type_shape` returns for an equal nominal application, so a Mod
/// compares them by handle equality; the signature is never a call or ownership grant.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticFunctionSignature {
    pub declaration: ModSemanticDeclaration,
    pub generic_count: u32,
    pub parameters: Vec<ModSemanticFieldType>,
    pub result: ModSemanticFieldType,
}

pub trait ModSemanticAuthority: super::ModSyntaxAuthority {
    /// Resolve `path` through canonical name resolution from an issued caller reference.
    /// Functions not visible from that caller (private items of other modules or packages,
    /// private inline modules outside the caller scope) are unresolvable.
    fn resolve_function(
        &self,
        _invocation: u64,
        _reference: &super::ModSyntaxNodeRef,
        _path: &[String],
    ) -> Result<ModSemanticFunctionSignature, ModSemanticError> {
        Err(ModSemanticError("authority cannot resolve caller-visible functions".into()))
    }
    /// The host serialization gate for an issued concrete handle: process and runtime-managed
    /// opaque resources that a Mod cannot observe through `type_shape` are rejected here.
    fn check_serializable(&self, _handle: ModSemanticHandle) -> Result<(), ModSemanticError> {
        Err(ModSemanticError("authority cannot check serialization eligibility".into()))
    }
    fn plan_canonical_paths(
        &self,
        _invocation: u64,
        _reference: &super::ModSyntaxNodeRef,
        _requested: &[String],
    ) -> Result<Vec<ModCanonicalPath>, ModSemanticError> {
        Err(ModSemanticError("authority cannot plan canonical source routes".into()))
    }
    fn generation(&self) -> SyntaxGenerationId;
    fn resolve_declaration(&self, declaration: &ModSemanticDeclaration) -> Result<ModSemanticHandle, ModSemanticError>;
    fn type_shape(&self, handle: ModSemanticHandle) -> Result<ModSemanticShape, ModSemanticError>;
    fn compile_serialization_target(
        &self,
        _invocation: u64,
        _contribution_index: u32,
        _target: ModSemanticHandle,
        _contribution: &crate::syntax::Spanned<super::ProgramItem>,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        Err(ModSemanticError("authority cannot issue typed serialization contribution correspondence".into()))
    }
    fn compile_serialization_template(
        &self,
        _invocation: u64,
        _contribution_index: u32,
        _reference: &super::ModSyntaxNodeRef,
        _declaration: &ModSemanticDeclaration,
        _contribution: &crate::syntax::Spanned<super::ProgramItem>,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        Err(ModSemanticError("authority cannot issue serialization template correspondence".into()))
    }
    fn compile_catchall(
        &self,
        _invocation: u64,
        _claim: &ModSemanticCatchallClaim,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        Err(ModSemanticError("authority cannot issue compiled catchall metadata".into()))
    }
    fn issue_catchall(
        &self,
        _invocation: u64,
        _owner: ModSemanticHandle,
        _field: &ModSemanticDeclaration,
    ) -> Result<ModSemanticCatchallClaim, ModSemanticError> {
        Err(ModSemanticError("authority cannot issue canonical catchall eligibility".into()))
    }
    fn lookup_catchall(&self, _invocation: u64, _token: u64) -> Result<ModSemanticCatchallClaim, ModSemanticError> {
        Err(ModSemanticError("authority cannot retrieve canonical catchall eligibility".into()))
    }
    fn validate_catchall_claim(
        &self,
        _invocation: u64,
        _claim: &ModSemanticCatchallClaim,
    ) -> Result<(), ModSemanticError> {
        Err(ModSemanticError("authority cannot validate canonical catchall eligibility".into()))
    }
}

/// Transport description, not an eligibility witness. Only its live invocation issuer
/// retains and validates the private field/application proof before contribution emission.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSemanticCatchallClaim {
    pub token: u64,
    pub owner: ModSemanticHandle,
    pub field: ModSemanticDeclaration,
    pub map: ModSemanticHandle,
    pub value: ModSemanticFieldType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModCompiledMetadataDescription {
    Catchall(ModSemanticCatchallClaim),
    SerializationTarget(ModSerializationTargetClaim),
    SerializationTemplate(ModSerializationTemplateClaim),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSerializationTargetClaim {
    pub contribution_index: u32,
    pub target: ModSemanticHandle,
    pub generation: SyntaxGenerationId,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSerializationTemplateClaim {
    pub contribution_index: u32,
    pub declaration: ModSemanticDeclaration,
    pub parameters: Vec<String>,
}
impl From<ModSerializationTemplateClaim> for ModCompiledMetadataDescription {
    fn from(claim: ModSerializationTemplateClaim) -> Self {
        Self::SerializationTemplate(claim)
    }
}
impl From<ModSemanticCatchallClaim> for ModCompiledMetadataDescription {
    fn from(claim: ModSemanticCatchallClaim) -> Self {
        Self::Catchall(claim)
    }
}
impl From<ModSerializationTargetClaim> for ModCompiledMetadataDescription {
    fn from(claim: ModSerializationTargetClaim) -> Self {
        Self::SerializationTarget(claim)
    }
}

/// Owned compiler metadata travels with a typed contribution, never across the native wire.
/// The description is informational. Consumers must recognize and revalidate the concrete
/// issuer's private payload; constructing this carrier cannot create an eligibility proof.
#[derive(Clone)]
pub struct ModCompiledMetadata {
    description: ModCompiledMetadataDescription,
    payload: std::sync::Arc<dyn std::any::Any + Send + Sync>,
}
impl ModCompiledMetadata {
    pub fn with_issuer_payload<T: std::any::Any + Send + Sync>(
        description: impl Into<ModCompiledMetadataDescription>,
        payload: T,
    ) -> Self {
        Self { description: description.into(), payload: std::sync::Arc::new(payload) }
    }
    pub fn description(&self) -> &ModCompiledMetadataDescription {
        &self.description
    }
    pub fn issuer_payload<T: std::any::Any>(&self) -> Option<&T> {
        self.payload.downcast_ref::<T>()
    }
}
impl std::fmt::Debug for ModCompiledMetadata {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ModCompiledMetadata").field("description", &self.description).finish_non_exhaustive()
    }
}

impl PartialEq for ModCompiledMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.description == other.description && std::sync::Arc::ptr_eq(&self.payload, &other.payload)
    }
}
impl Eq for ModCompiledMetadata {}
