//! One module per `beskid` subcommand; each exposes an `execute` function consumed by [`crate::cli::run`].

pub mod analyze;
pub mod build;
pub mod clif;
pub mod compiler_mod;
pub mod corelib;
pub mod doc;
pub mod fetch;
pub mod format;
pub mod graph;
pub mod hi;
pub mod hi_compile;
pub mod import;
pub mod lock;
pub mod lsp;
pub mod matrix_test;
pub mod migrate_bsol;
pub mod new;
pub mod parse;
pub mod repl;
pub mod run;
pub mod runtime_kit;
mod syntax_codegen;
pub mod test;
pub mod tree;
pub mod update;
pub mod validate_bsol;
