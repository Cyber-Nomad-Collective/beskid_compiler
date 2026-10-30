//! [`serve_stdio`]: the language server on stdin and stdout.

use std::sync::Arc;

use tower_lsp_server::{LspService, Server};

use super::backend::Backend;
use super::lifetime::{ExitHook, ExitOnEof};
use super::ordered_service::TextSyncOrderedService;
use crate::crash::{self, CrashReporter};

/// Messages the transport evaluates at once. A text-sync notification runs the analysis of its
/// edit inside its slot and may wait for its project's database, so the transport default of
/// four slots let four slow edits stop the server from reading even syntax-only requests.
/// Ordering does not depend on this limit (see `TextSyncOrderedService`); each project's
/// database gate bounds the analysis work.
const MAX_IN_FLIGHT_MESSAGES: usize = 128;

/// Run the language server on stdio until the client exits: the start-up shared by `beskid_lsp`
/// and `beskid dev lsp`. Call it from a plain thread with no async runtime running; it limits the
/// allocator before it starts the runtime's threads.
pub fn serve_stdio() -> anyhow::Result<()> {
    limit_malloc_arenas();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let served = runtime.block_on(run_stdio_server());
    // Dropping the runtime would wait for blocking work in flight (an analysis, an index step).
    runtime.shutdown_background();
    served
}

/// glibc gives each thread that allocates concurrently its own malloc arena (up to eight per
/// core) and returns little of their freed memory. The server allocates and frees whole typed
/// programs on many threads, so its resident size grew to twice what it used (measured in a
/// Corelib soak: 12 GB with the default arenas, 5.6 GB with two, at the same latency).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn limit_malloc_arenas() {
    // SAFETY: `mallopt` only adjusts allocator tuning, and it runs before the runtime starts any
    // other thread.
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn limit_malloc_arenas() {}

async fn run_stdio_server() -> anyhow::Result<()> {
    // The LSP may be the first Beskid executable a user runs. Provision the
    // Corelib embedded in this binary before workspace resolution begins, so
    // editor diagnostics use the release bundle rather than a source checkout.
    beskid_tools::configure_corelib_root_from_environment()?;
    beskid_tools::ensure_bundled_corelib(&beskid_corelib_embed::CORELIB_BUNDLE)?;
    // The process ends at once on `exit`, at the end of stdin and when the client process is
    // gone (see `server::lifetime`); nothing waits for in-flight analyses or the index.
    let exit: ExitHook = Arc::new(|code| std::process::exit(code));
    let stdin = ExitOnEof::new(tokio::io::stdin(), Arc::clone(&exit));
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
    Server::new(stdin, stdout, socket)
        .concurrency_level(MAX_IN_FLIGHT_MESSAGES)
        .serve(TextSyncOrderedService::new(service, reporter).ending_the_process_with(exit))
        .await;
    Ok(())
}
