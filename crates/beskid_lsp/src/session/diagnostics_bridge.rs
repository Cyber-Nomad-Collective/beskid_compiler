//! Shared helper for Salsa-backed diagnostics from LSP state.

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use beskid_analysis::CompilationContext;
use beskid_analysis::services::DocumentAnalysisSnapshot;

use crate::diagnostics::analyze_document;
use crate::session::db_access::with_compilation_db_mut_state;
use crate::session::store::State;

pub async fn analyze_document_for_state(
    state: &RwLock<State>,
    uri: &Uri,
    source: &str,
    cached: Option<&DocumentAnalysisSnapshot>,
    compilation_context: Option<&CompilationContext>,
) -> Vec<tower_lsp_server::ls_types::Diagnostic> {
    with_compilation_db_mut_state(state, |db, write| {
        if let Some(ctx) = compilation_context
            && let Some(plan) = ctx.compile_plan.as_ref()
        {
            write.configure_db_for_project(&plan.project_root);
        }
        analyze_document(Some(db), uri, source, cached, compilation_context)
    })
    .await
}
