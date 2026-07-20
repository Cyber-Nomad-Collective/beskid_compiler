//! Shared helper for Salsa-backed diagnostics from LSP state.
//!
//! Publish/refresh prefer generation-bound `Document.syntax_diagnostics` facts rebuilt via
//! lifecycle. This bridge remains for one-shot callers that need a Salsa-backed collect.

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::diagnostics::{collect_syntax_diagnostics, lsp_diagnostics_from_syntax};
use crate::session::db_access::with_compilation_db_mut_state;
use crate::session::startup::wait_for_initial_scan;
use crate::session::store::{State, SyntaxDiagnostic};
use beskid_analysis::CompilationContext;

/// Collect generation-bound diagnostic facts using the shared Salsa database.
pub async fn collect_syntax_diagnostics_for_state(
    state: &RwLock<State>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<SyntaxDiagnostic> {
    wait_for_initial_scan(state).await;

    with_compilation_db_mut_state(state, |db, write| {
        if let Some(ctx) = compilation_context
            && let Some(plan) = ctx.compile_plan.as_ref()
        {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        collect_syntax_diagnostics(Some(db), uri, source, compilation_context)
    })
    .await
}

/// One-shot LSP diagnostics via the shared database (does not read analysis snapshots).
#[allow(dead_code)]
pub async fn analyze_document_for_state(
    state: &RwLock<State>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<tower_lsp_server::ls_types::Diagnostic> {
    let facts =
        collect_syntax_diagnostics_for_state(state, uri, source, compilation_context).await;
    lsp_diagnostics_from_syntax(source, &facts)
}
