//! Closure environment, capture, spawn, and fiber-ownership facts.

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

/// One exact outer lexical declaration captured by a lambda or spawned lambda.
///
/// `class` and `span` come from the first captured use site under the lambda in syntax-index
/// order. They preserve capture mode and source identity without reconstructing HIR snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureCapture {
    pub declaration: AstNodeKey,
    pub slot: LocalSlot,
    pub class: CaptureStorageClass,
    pub span: SourceSpan,
}

/// Backend-relevant closure environment facts derived from one lambda expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureEnvironment {
    pub parameters: Arc<[AstNodeKey]>,
    pub captures: Arc<[ClosureCapture]>,
}

/// One deterministic capture field in a target-neutral closure environment ABI shape.
///
/// Field order follows the captured declaration's stable owner/node identity and local slot,
/// never hash-map iteration or a later codegen traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureEnvironmentField {
    pub capture: ClosureCapture,
    pub abi_type: SemanticTypeId,
}

/// Requirement that a closure environment descriptor carry a runtime pointer map.
///
/// This is intentionally a requirement, not a claim that a descriptor has been emitted. The
/// query layer has no runtime allocation or descriptor-emission authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ClosurePointerMapRequirement {
    RuntimeDescriptorRequired,
}

/// Deterministic target-neutral ABI shape for a lambda's capture environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureEnvironmentAbiShape {
    pub fields: Arc<[ClosureEnvironmentField]>,
    pub pointer_map: ClosurePointerMapRequirement,
}

/// Current implementation status for consuming closure facts in generated lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ClosureLoweringStatus {
    NotLowered,
}

/// Current implementation status for creating a closure environment at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ClosureAllocationStatus {
    NotAllocated,
}

/// Generation-bound callable and environment facts for one lambda expression.
///
/// Generic/inferred callable forms remain unavailable. This fact records no generated lowering
/// or runtime allocation; those statuses remain explicit until codegen owns them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureSignature {
    pub lambda: AstNodeKey,
    pub body: AstNodeKey,
    pub callable: ItemSignature,
    pub environment: ClosureEnvironmentAbiShape,
    pub lowering: ClosureLoweringStatus,
    pub allocation: ClosureAllocationStatus,
}

/// Direct lambda call selected by a current call expression.
///
/// Calls through a local closure binding remain unavailable: syntax facts do not infer an
/// allocation, binding flow, or dynamic dispatch target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ClosureCallTarget {
    pub call: AstNodeKey,
    pub lambda: AstNodeKey,
    pub body: AstNodeKey,
    pub callable: ItemSignature,
}

/// Exact callable operand, eager arguments, and captures selected by a `spawn` expression.
///
/// `spawn Entry()` and `spawn Entry(args)` both store the entry operand (path or lambda), never
/// the CallExpression. `arguments` holds the normalized argument expressions in source order;
/// the parent evaluates them before the spawn and transfers them in the fiber's start
/// environment. Only direct item entries may take arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnTarget {
    pub callee: AstNodeKey,
    pub arguments: Arc<[AstNodeKey]>,
    pub captures: Arc<[ClosureCapture]>,
}

/// Storage provenance derived from the current syntax authority for one captured local use.
///
/// This fact does not establish closure rooting or allocation. It only classifies source values
/// that are safe to transfer by value; native pointers and mutable bindings are conservatively
/// stack references, because moving either across a fiber boundary can expose an invalid or
/// aliased stack location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CaptureStorageClass {
    TransferableValue,
    StackReference,
}

/// Exact declaration, storage provenance, and source use for one captured local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CaptureStorage {
    pub declaration: AstNodeKey,
    pub class: CaptureStorageClass,
    pub span: SourceSpan,
}

/// Deterministic syntax-owned legality failure for one spawn expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SpawnDiagnosticKind {
    TargetNotCallable,
    TargetRequiresArguments,
    /// `spawn callee(args)` whose callee is not a direct item (for example a lambda or a local
    /// closure value); only direct item entries receive eager spawn arguments.
    CalleeArgumentsUnsupported,
    StackReferenceEscapesSpawn,
    DiscardedHandle,
    UseAfterMove,
}

/// One precise diagnostic selected from current syntax facts for a spawn expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnDiagnostic {
    pub kind: SpawnDiagnosticKind,
    pub span: SourceSpan,
    pub capture: Option<CaptureStorage>,
}

/// Move-only Fiber capabilities in one callable, bound to the current syntax generation.
/// Spawn legality and emission both consume this fact, including callables without a spawn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FiberOwnership {
    pub callable: AstNodeKey,
    pub diagnostics: Arc<[SpawnDiagnostic]>,
}

/// Authoritative spawn lowering facts and any source-owned legality diagnostics.
///
/// A legal fact contains a zero-argument callable signature result and no diagnostics. Illegal
/// facts retain the target and any proven result so diagnostics and lowering never need legacy
/// HIR snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnLegality {
    pub target: SpawnTarget,
    pub result: Option<SemanticTypeId>,
    pub span: SourceSpan,
    pub diagnostics: Arc<[SpawnDiagnostic]>,
}

/// Nominal handle application projected from the authoritative spawn legality fact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnHandleType {
    pub declaration: AstNodeKey,
    pub payload: GenericSubstitution,
}

/// Source-only validation of whether a spawn target is a legal fiber entry.
///
/// `arguments` are the eager argument expressions, one per `callable` parameter when
/// `is_legal_entry` holds. This mirrors current legality facts without claiming that a fiber
/// trampoline, closure allocation, or runtime scheduling object has been generated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpawnEntryValidation {
    pub spawn: AstNodeKey,
    pub target: AstNodeKey,
    pub arguments: Arc<[AstNodeKey]>,
    pub callable: Option<ItemSignature>,
    pub is_legal_entry: bool,
    pub diagnostics: Arc<[SpawnDiagnostic]>,
}

impl SpawnLegality {
    pub fn is_legal(&self) -> bool {
        self.diagnostics.is_empty() && self.result.is_some()
    }
}
