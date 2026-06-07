//! Opinionated pretty-printer (`Emit` trait), mirroring bsharp layout rules.

mod emit;
mod expressions_emit;
mod items;
mod naming_normalize;
mod policy;
mod statements_emit;
mod types_emit;

pub use emit::{Emit, EmitCtx, EmitError, Emitter, emit_error_semantic_diagnostic, format_program};
