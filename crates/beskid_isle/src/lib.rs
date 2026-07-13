//! Generated ISLE selection for Beskid's stock CLIF lowering path.

pub use beskid_queries::AstNodeKey;
pub use cranelift_codegen::ir::Value;

pub const ISLE_INPUTS: &[&str] = &[
    "types.isle",
    "ast.isle",
    "expressions.isle",
    "literals.isle",
    "binary.isle",
    "unary_casts.isle",
    "calls.isle",
    "statements.isle",
    "control_flow.isle",
    "memory.isle",
    "runtime_intrinsics.isle",
    "items.isle",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    IntegerLiteral,
    Unsupported,
}

#[allow(
    unused_imports,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::len_without_is_empty
)]
pub mod generated {
    use super::{AstNodeKey, NodeKind, Value};

    include!(concat!(env!("OUT_DIR"), "/beskid_lower.rs"));
}

include!(concat!(env!("OUT_DIR"), "/beskid_isle_metadata.rs"));
