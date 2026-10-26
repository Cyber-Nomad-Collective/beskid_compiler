//! Shared helper for Salsa-backed diagnostics from LSP state.
//!
//! Publish/refresh prefer generation-bound `Document.syntax_diagnostics` facts rebuilt via
//! lifecycle. This bridge remains for one-shot callers that need a Salsa-backed collect.

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::diagnostics::{ProjectPreparation, collect_syntax_diagnostics, lsp_diagnostics_from_syntax};
use crate::session::db_access::with_project_db;
use crate::session::startup::wait_for_initial_scan;
use crate::session::store::{State, SyntaxDiagnostic};
use beskid_analysis::{CompilationContext, SyntaxFix};

/// Collect generation-bound diagnostic facts and mod-origin quick-fixes using the project's
/// Salsa database. Returns `(diagnostics, fixes)` so the lifecycle can store both on the
/// `Document` at the same generation.
pub async fn collect_syntax_diagnostics_for_state(
    state: &RwLock<State>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> (Vec<SyntaxDiagnostic>, Vec<SyntaxFix>) {
    wait_for_initial_scan(state).await;

    let (mod_host, interactive) = {
        let read = state.read().await;
        (read.mod_host.clone(), read.docs.contains_key(uri))
    };
    let project_root = compilation_context.and_then(|ctx| ctx.compile_plan.as_ref()).map(|plan| &plan.project_root);
    with_project_db(state, project_root.map(|root| root.as_path()), interactive, |db| {
        collect_syntax_diagnostics(
            Some(ProjectPreparation { db, mod_host: &mod_host }),
            uri,
            source,
            compilation_context,
        )
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
    let (facts, _fixes) = collect_syntax_diagnostics_for_state(state, uri, source, compilation_context).await;
    lsp_diagnostics_from_syntax(source, &facts)
}
