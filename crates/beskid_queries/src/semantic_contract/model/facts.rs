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

/// Generation-bound iterator declaration and element type for one `ForStatement`.
///
/// The declaration is the loop-variable identifier. Element type is proven only for the
/// syntax-only `range(start, end)` iterable; other iterables remain unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ForIteratorFact {
    pub declaration: AstNodeKey,
    pub element_type: SemanticTypeId,
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

/// Callable item signature expressed entirely in semantic type identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ItemSignature {
    pub parameters: Arc<[SemanticTypeId]>,
    pub result: SemanticTypeId,
}
