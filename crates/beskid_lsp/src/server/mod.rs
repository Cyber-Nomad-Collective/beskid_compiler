//! LSP server surface: [`backend::Backend`], initialization helpers, the text-sync ordered
//! request service and the stdio entry point.

pub mod backend;
pub(crate) mod init;
pub(crate) mod lifetime;
pub(crate) mod ordered_service;
pub mod stdio;

#[cfg(test)]
mod init_capabilities_tests;
