use tokio::sync::RwLock;
use tower_lsp_server::{Client, ls_types::Uri};

use crate::session::db_access::document_update_gate;
use crate::{diagnostics::lsp_diagnostics_from_syntax, session::store::State};

use super::documents::build_full_diagnostic_facts;

fn apply_if_current(
    state: &mut State,
    uri: &Uri,
    version: i32,
    text: &str,
    diagnostics: Vec<crate::session::store::SyntaxDiagnostic>,
    fixes: Vec<beskid_analysis::SyntaxFix>,
) -> bool {
    if let Some(open) = state.docs.get_mut(uri) {
        if open.version != version || open.text != text {
            return false;
        }
        open.syntax_diagnostics = diagnostics;
        open.syntax_fixes = fixes;
        return true;
    }
    if let Some(indexed) = state.workspace_index.get_mut(uri) {
        if indexed.version != version || indexed.text != text {
            return false;
        }
        indexed.syntax_diagnostics = diagnostics;
        indexed.syntax_fixes = fixes;
        return true;
    }
    false
}

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
    let (diagnostic_facts, fixes) = build_full_diagnostic_facts(state, uri, &text).await;
    let diagnostics = lsp_diagnostics_from_syntax(&text, &diagnostic_facts);
    // Serialize the final identity check with this document's mutation only.
    // Release the fence before client I/O: the version-tagged publication can be
    // discarded by the client, while a slow transport must not stall text sync.
    let update_gate = document_update_gate(state, uri).await;
    let _update_guard = update_gate.lock().await;
    {
        let mut write = state.write().await;
        if !apply_if_current(&mut write, uri, version, &text, diagnostic_facts, fixes) {
            return;
        }
    }
    drop(_update_guard);
    client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tower_lsp_server::ls_types::Uri;

    use super::apply_if_current;
    use crate::session::store::SyntaxDiagnostic;
    use crate::session::store::{Document, State};

    fn empty_document(version: i32, text: &str) -> Document {
        Document {
            version,
            text: text.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            bsol_semantic_token_candidates: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
            syntax_fixes: Vec::new(),
        }
    }

    #[test]
    fn same_text_newer_version_rejects_stale_diagnostic_facts() {
        let uri = Uri::from_str("file:///tmp/Main.bd").expect("URI");
        let mut state = State::default();
        state.docs.insert(uri.clone(), empty_document(2, "i32 Main() { return 0; }"));

        let empty = || (Vec::<SyntaxDiagnostic>::new(), Vec::<beskid_analysis::SyntaxFix>::new());
        let (diagnostics, fixes) = empty();
        assert!(!apply_if_current(&mut state, &uri, 1, "i32 Main() { return 0; }", diagnostics, fixes,));
        let (diagnostics, fixes) = empty();
        assert!(apply_if_current(&mut state, &uri, 2, "i32 Main() { return 0; }", diagnostics, fixes,));
    }
}
