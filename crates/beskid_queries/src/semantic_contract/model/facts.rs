//! Cast, control-flow, loop, bulk, try, and item-signature facts.

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

/// One semantic cast required while lowering an AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CastIntent {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PrimitiveNumericConversion {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

/// Control-flow facts established for one AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ControlFlow {
    pub may_fall_through: bool,
}

/// Exact source bounds for the built-in `range(start, end)` form consumed by a `for` loop.
///
/// This is deliberately separate from ordinary call lowering: `range` is syntax sugar for the
/// loop emitter, not a dynamically dispatched function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RangeForFact {
    pub start: AstNodeKey,
    pub end: AstNodeKey,
}

/// What a `for` statement iterates: the syntax-only `range(start, end)` form, an array value
/// whose element identity the source proves (`T[]` parameters and locals, field projections,
/// call results, array literals), or a nominal value of the iterator protocol (a type whose
/// unique `Next()` method takes no parameters and returns `Option<T>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ForIterableSource {
    Range,
    /// The loop reads every element of the array value evaluated once at loop entry, in index
    /// order. `element_managed` is the GC classification of the elements (the loop variable's
    /// root discipline), derived from the element's source identity, never from its ABI.
    Array { element_managed: super::types::ManagedReferenceKind },
    /// The loop calls `Next()` on the iterator value evaluated once at loop entry until it
    /// returns `None`, binding each `Some` payload. The callee and `Option` layout are the
    /// [`ForIteratorProtocol`] of the statement; `element_managed` classifies the payloads.
    Iterator { element_managed: super::types::ManagedReferenceKind },
}

/// Lowering authority of one `for` over the nominal iterator protocol.
///
/// `next` is the exact `Next()` instance the loop calls with the iterator as its receiver (empty
/// substitutions for a non-generic owner; the owner's bindings for a generic one). `option` and
/// `option_layout` describe the `Option<T>` it returns, `some_variant` is the source-order index
/// of its `Some` variant, and the element is that variant's single payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForIteratorProtocol {
    pub next: super::generics::GenericSpecializationInstance,
    pub option: AstNodeKey,
    pub option_layout: super::layouts::EnumLayoutFact,
    pub some_variant: u32,
    pub element_type: SemanticTypeId,
    pub element_managed: super::types::ManagedReferenceKind,
}

/// Generation-bound iterator declaration, element type and source for one `ForStatement`.
///
/// The declaration is the loop-variable identifier. The element type is the start type of a
/// `range(start, end)` iterable, the element ABI of an array iterable, or the `Some` payload ABI
/// of a nominal iterator; anything else remains unavailable (the check gate judges unproven
/// iterables as E1215).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ForIteratorFact {
    pub declaration: AstNodeKey,
    pub element_type: SemanticTypeId,
    pub source: ForIterableSource,
}

/// Source-proven bulk calling-convention marker for one function or method parameter.
///
/// A `bulk` parameter still lowers as a single array parameter at the callee (the signature
/// shape is unchanged); this fact only marks *which* parameter is bulk and its declared
/// element ABI type, so call-site lowering can pack N scalar arguments into a fresh rooted
/// array before the direct call. The parameter index is the position of the parameter in its
/// enclosing callable's parameter list (declaration order). Stale, unregistered, non-parameter
/// nodes, and parameters without the `bulk` modifier contain no fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BulkParameterFact {
    pub parameter: AstNodeKey,
    pub parameter_index: u32,
    pub element_abi_type: SemanticTypeId,
}

/// Syntax-proven payload/error shapes for one postfix `Result` propagation expression.
///
/// The operand is a typed parameter or a proven ordinary direct call. Both Result
/// instantiations share one declaration and the exact error source identity; their
/// success identities and physical layouts may differ.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TryExpressionFact {
    pub expression: AstNodeKey,
    pub operand: AstNodeKey,
    pub(in crate::semantic_contract) payload_identity: GenericSourceTypeIdentity,
    pub payload_type: SemanticTypeId,
    pub error_type: SemanticTypeId,
    pub enclosing_return: SemanticTypeId,
    pub operand_layout: EnumLayoutFact,
    pub return_layout: EnumLayoutFact,
    pub error_managed: bool,
}
impl TryExpressionFact {
    /// Whether the propagated payload is a managed reference (a record, enum or string payload).
    pub fn payload_managed_reference_kind(&self) -> ManagedReferenceKind {
        self.payload_identity.managed_reference_kind()
    }
}

/// Callable item signature expressed entirely in semantic type identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ItemSignature {
    pub parameters: Arc<[SemanticTypeId]>,
    pub result: SemanticTypeId,
}
