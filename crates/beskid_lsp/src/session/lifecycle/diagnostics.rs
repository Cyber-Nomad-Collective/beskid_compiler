use tokio::sync::RwLock;
use tower_lsp_server::{Client, ls_types::Uri};

use crate::{diagnostics::lsp_diagnostics_from_syntax, session::store::State};

use super::documents::{apply_syntax_facts, build_syntax_facts};

/// Refresh generation-bound diagnostic facts for the open buffer or workspace snapshot and push
/// to the client. Never reads `Document.analysis` / HIR snapshots.
pub async fn publish_diagnostics_for_uri(client: &Client, state: &RwLock<State>, uri: &Uri) {
    let snapshot = {
        let state = state.read().await;
        state.document_union(uri)
    };

    let Some(doc) = snapshot else {
        return;
    };

    let text = doc.text.clone();
    let version = doc.version;
    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    let diagnostics = lsp_diagnostics_from_syntax(&text, &syntax_facts.diagnostics);
    {
        let mut write = state.write().await;
        if let Some(open) = write.docs.get_mut(uri)
            && open.text == text
        {
            apply_syntax_facts(open, syntax_facts);
        } else if let Some(indexed) = write.workspace_index.get_mut(uri)
            && indexed.text == text
        {
            apply_syntax_facts(indexed, syntax_facts);
        }
    }
    client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
}
