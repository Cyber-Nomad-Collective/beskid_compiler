//! `beskid lsp` — run the Beskid language server on stdio (editor integration).

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct LspArgs {}

/// Start the LSP server on stdin/stdout.
pub fn execute(_args: LspArgs) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(beskid_lsp::run_stdio_server())
}
