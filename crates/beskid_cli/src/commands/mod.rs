//! One module per `beskid` subcommand; each exposes an `execute` function consumed by [`crate::cli::run`].

pub mod analyze;
pub mod build;
pub mod clif;
pub mod compiler_mod;
pub mod corelib;
pub mod doc;
pub mod fetch;
pub mod format;
pub mod import;
pub mod lock;
pub mod new;
pub mod parse;
pub mod run;
pub mod test;
pub mod tree;
pub mod update;
