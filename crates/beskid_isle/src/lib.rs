//! Generated ISLE selection for Beskid's stock CLIF lowering path.
//!
//! Raw constructors are an implementation detail of [`FunctionEmitter`]:
//!
//! ```compile_fail
//! use beskid_isle::generated;
//! ```

pub use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, MemFlags, Signature, StackSlotData, StackSlotKind, TrapCode, Type,
    UserFuncName, Value,
};

mod clif_primitives;
mod context;
mod dispatch;
mod emitter;
mod errors;
mod facts;
mod layout;

pub use clif_primitives::ClifPrimitives;
pub use context::{CallImporter, IsleContext, StringInterner, lower_expression, lower_statement};
pub use dispatch::pointer_type as isle_pointer_type;
pub use emitter::{EmissionServices, FunctionEmitter, ItemStatementEmission};
pub use errors::{FunctionEmissionError, LoweringError, LoweringErrorKind, StringMaterializationError};
pub use facts::*;
pub use layout::*;

include!(concat!(env!("OUT_DIR"), "/beskid_isle_metadata.rs"));

/// Stable syntax-facing grouping for callers that do not need the full lowering contract.
pub mod syntax_types {
    pub use crate::{
        CallKind, ForIterableKind, IndexTarget, LiteralKind, OperatorFact, RuntimeIntrinsicKind,
        SyntaxNodeClassification, UNSUPPORTED_TYPED_OPERATION_KINDS, classify_syntax_node_kind,
        syntax_node_kind_catalogue, unsupported_typed_operation_kinds,
    };
}

/// Stable callee contract grouping for importer implementations.
pub mod callee {
    pub use crate::DirectCallee;
}
