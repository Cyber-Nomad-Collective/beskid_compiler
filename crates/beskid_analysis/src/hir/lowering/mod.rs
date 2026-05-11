//! Syntax → HIR lowering (single pass over the AST-shaped [`crate::hir::AstProgram`]).

mod core;

pub use core::lower_program;
