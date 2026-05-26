//! LSP server surface: [`backend::Backend`] and initialization helpers.

pub mod backend;
pub(crate) mod init;

#[cfg(test)]
mod init_capabilities_tests;
