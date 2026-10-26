//! [`run_stdio_server`]: the language server on stdin and stdout.

use std::sync::Arc;

use tower_lsp_server::{LspService, Server};

use super::backend::Backend;
use super::ordered_service::TextSyncOrderedService;
use crate::crash::{self, CrashReporter};

/// Run the language server on stdio (used by `beskid_lsp` and `beskid dev lsp`).
pub async fn run_stdio_server() -> anyhow::Result<()> {
    // The LSP may be the first Beskid executable a user runs. Provision the
    // Corelib embedded in this binary before workspace resolution begins, so
    // editor diagnostics use the release bundle rather than a source checkout.
    beskid_tools::ensure_bundled_corelib(&beskid_corelib_embed::CORELIB_BUNDLE)?;
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let reporter = Arc::new(CrashReporter::for_home());
    let notifier_reporter = Arc::clone(&reporter);
    let (service, socket) = LspService::new(move |client| {
        let notify_client = client.clone();
        notifier_reporter.set_notifier(move |params| {
            let client = notify_client.clone();
            Box::pin(async move { client.send_notification::<crash::CrashReport>(params).await })
        });
        Backend::new(client, Arc::clone(&notifier_reporter))
    });
    Server::new(stdin, stdout, socket).serve(TextSyncOrderedService::new(service, reporter)).await;
    Ok(())
}
