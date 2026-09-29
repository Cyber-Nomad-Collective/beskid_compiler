use tokio::sync::RwLock;
use tower_lsp_server::{Client, ls_types::Uri};

use super::documents::build_full_diagnostic_facts;
use crate::{
    diagnostics::lsp_diagnostics_from_syntax,
    session::{db_access::document_update_gate, store::State},
};

fn apply_if_current(
    state: &mut State,
    uri: &Uri,
    version: i32,
    text: &str,
    generation: u64,
    diagnostics: Vec<crate::session::store::SyntaxDiagnostic>,
    fixes: Vec<beskid_analysis::SyntaxFix>,
) -> bool {
    if state.diagnostic_generation != generation {
        return false;
    }
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
        (state.document_union(uri), state.diagnostic_generation)
    };

    let (Some(doc), generation) = snapshot else {
        return;
    };

    let text = doc.text.clone();
    let version = doc.version;
    let (diagnostic_facts, fixes) = build_full_diagnostic_facts(state, uri, &text).await;
    let diagnostics = lsp_diagnostics_from_syntax(&text, &diagnostic_facts);
    // Serialize the final identity check with this document's mutation only.
    // Hold this URI's publication fence through client enqueue. Otherwise an older
    // completion may pass the identity check, yield during transport, and arrive
    // after the refreshed lock-generation publication.
    let update_gate = document_update_gate(state, uri).await;
    let _update_guard = update_gate.lock().await;
    {
        let mut write = state.write().await;
        if !apply_if_current(&mut write, uri, version, &text, generation, diagnostic_facts, fixes) {
            return;
        }
    }
    client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tokio::sync::RwLock;
    use tower_lsp_server::ls_types::Uri;

    use super::apply_if_current;
    use crate::session::{
        project_context::invalidate_compilation_cache,
        store::{Document, State, SyntaxDiagnostic, SyntaxDiagnosticSeverity},
    };

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
        let generation = state.diagnostic_generation;

        let empty = || (Vec::<SyntaxDiagnostic>::new(), Vec::<beskid_analysis::SyntaxFix>::new());
        let (diagnostics, fixes) = empty();
        assert!(!apply_if_current(&mut state, &uri, 1, "i32 Main() { return 0; }", generation, diagnostics, fixes,));
        let (diagnostics, fixes) = empty();
        assert!(apply_if_current(&mut state, &uri, 2, "i32 Main() { return 0; }", generation, diagnostics, fixes,));
    }

    #[tokio::test]
    async fn same_text_and_version_cannot_restore_diagnostics_from_before_lock_invalidation() {
        let uri = Uri::from_str("file:///tmp/Main.bd").expect("URI");
        let state = RwLock::new(State::default());
        let text = "i32 Main() { return 0; }";
        state.write().await.docs.insert(uri.clone(), empty_document(2, text));

        // Model the deterministic interleaving: an earlier debounced diagnostic has
        // captured the same buffer version, then a watched Project.lock change
        // invalidates the compilation generation before that result completes.
        let old_diagnostic = SyntaxDiagnostic {
            start: 0,
            end: 0,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some("old".into()),
            message: "old compilation generation".into(),
            source: "beskid".into(),
        };
        let old_generation = state.read().await.diagnostic_generation;
        invalidate_compilation_cache(&state).await;
        let mut write = state.write().await;
        assert!(!apply_if_current(&mut write, &uri, 2, text, old_generation, vec![old_diagnostic], Vec::new()));
        assert!(write.docs[&uri].syntax_diagnostics.is_empty());
    }
}
