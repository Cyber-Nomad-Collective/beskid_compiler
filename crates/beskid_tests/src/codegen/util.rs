//! Re-exports from `support::pipeline` for codegen tests.

pub use crate::support::pipeline::{
    parse_program as parse_program_ast, typecheck_hir as lower_resolve_type,
};
