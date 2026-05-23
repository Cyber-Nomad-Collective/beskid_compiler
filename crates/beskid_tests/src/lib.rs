//! Integration tests for the Beskid compiler workspace (`cargo test -p beskid_tests`).
//!
//! Modules are `#[cfg(test)]`-gated helpers and suites: parsing, analysis, AOT, projects, LSP, and more.

#[cfg(test)]
pub mod format;

#[cfg(test)]
mod doc_tests;

#[cfg(test)]
mod test_harness;

#[cfg(test)]
pub mod parsing;

#[cfg(test)]
pub mod syntax;

#[cfg(test)]
pub mod analysis;

#[cfg(test)]
pub mod runtime;

#[cfg(test)]
pub mod codegen;

#[cfg(test)]
pub mod composition;

#[cfg(test)]
pub mod projects;

#[cfg(test)]
pub mod abi;

#[cfg(test)]
pub mod aot;

#[cfg(test)]
pub mod lsp;

#[cfg(test)]
pub mod cli;

#[cfg(test)]
pub mod interop;

#[cfg(test)]
pub mod template;

#[cfg(test)]
pub mod mods;

#[cfg(all(test, feature = "pckg"))]
pub mod pckg;
