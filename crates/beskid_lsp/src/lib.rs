//! Beskid Language Server: document sync, semantic features, and workspace-wide indexing.

pub(crate) mod adapters;
pub(crate) mod commands;
pub(crate) mod diagnostics;
pub(crate) mod features;
pub(crate) mod logging;
pub(crate) mod position;
pub(crate) mod protocol;
pub mod server;
pub(crate) mod session;
pub(crate) mod text_sync;
pub(crate) mod workspace_scan;

use server::backend::Backend;
use tower_lsp_server::{LspService, Server};

/// Run the language server on stdio (used by `beskid_lsp` and `beskid lsp`).
pub async fn run_stdio_server() -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}
