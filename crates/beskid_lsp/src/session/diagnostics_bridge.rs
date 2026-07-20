//! Shared helper for Salsa-backed diagnostics from LSP state.

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::diagnostics::analyze_document;
use crate::session::db_access::with_compilation_db_mut_state;
use crate::session::startup::wait_for_initial_scan;
use crate::session::store::State;
use beskid_analysis::CompilationContext;

pub async fn analyze_document_for_state(
    state: &RwLock<State>,
    uri: &Uri,
    source: &str,
    compilation_context: Option<&CompilationContext>,
) -> Vec<tower_lsp_server::ls_types::Diagnostic> {
    wait_for_initial_scan(state).await;

    with_compilation_db_mut_state(state, |db, write| {
        if let Some(ctx) = compilation_context
            && let Some(plan) = ctx.compile_plan.as_ref()
        {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        analyze_document(Some(db), uri, source, compilation_context)
    })
    .await
}
