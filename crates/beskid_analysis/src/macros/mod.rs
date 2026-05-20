//! Language macro expansion (`macro.expand`): typed AST substitution for `name!` invocations.

mod expand;
mod registry;
mod substitute;

pub use expand::{expand_program, DEFAULT_MAX_MACRO_EXPANSION_DEPTH};
