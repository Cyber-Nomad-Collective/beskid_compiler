//! Opinionated pretty-printer (`Emit` trait), mirroring bsharp layout rules.

mod emit;
mod expressions_emit;
mod items_emit;
mod policy;
mod statements_emit;
mod types_emit;

pub use emit::{Emit, EmitCtx, EmitError, Emitter, format_program};
