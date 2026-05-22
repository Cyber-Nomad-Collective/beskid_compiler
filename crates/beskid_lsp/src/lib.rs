//! Beskid Language Server: document sync, semantic features, and workspace-wide indexing.

pub(crate) mod adapters;
pub(crate) mod diagnostics;
pub(crate) mod features;
pub(crate) mod logging;
pub(crate) mod position;
pub(crate) mod protocol;
pub mod server;
pub(crate) mod session;
pub(crate) mod text_sync;
pub(crate) mod project_explorer_api;
pub(crate) mod workspace_scan;
