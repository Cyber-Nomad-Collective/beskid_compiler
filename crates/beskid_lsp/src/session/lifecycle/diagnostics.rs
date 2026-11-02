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
    let span = crate::trace::Span::start("publish", || format!("uri={} version={version}", uri.as_str()));
    let Some((diagnostic_facts, fixes)) = build_full_diagnostic_facts(state, uri, &text).await else {
        drop(span);
        return;
    };
    drop(span);
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
        if write
            .published_diagnostics
            .get(uri)
            .is_some_and(|(known, published)| *known == version && *published != diagnostics)
        {
            crate::trace::event("publish-corrects", || format!("uri={} version={version}", uri.as_str()));
        }
        // The analysis of this text published the same diagnostics already.
        if !record_publication(&mut write, uri, version, &diagnostics) {
            crate::trace::event("publish-same", || format!("uri={} version={version}", uri.as_str()));
            return;
        }
    }
    client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
}

/// Remember `diagnostics` as published for an open `uri` at `version`; `false` when exactly these
/// were published for that version already (nothing to send).
fn record_publication(
    state: &mut State,
    uri: &Uri,
    version: i32,
    diagnostics: &[tower_lsp_server::ls_types::Diagnostic],
) -> bool {
    if !state.docs.contains_key(uri) {
        return true;
    }
    if state
        .published_diagnostics
        .get(uri)
        .is_some_and(|(known, published)| *known == version && published == diagnostics)
    {
        return false;
    }
    state.published_diagnostics.insert(uri.clone(), (version, diagnostics.to_vec()));
    true
}

/// The diagnostics the analysis of the newest text of an open document stored with its facts,
/// with that text's version: `None` when the stored facts are not those of the newest open text
/// (a newer edit is being analyzed, or the document was closed).
fn analyzed_diagnostics(state: &State, uri: &Uri) -> Option<(i32, Vec<tower_lsp_server::ls_types::Diagnostic>)> {
    let doc = state.docs.get(uri)?;
    match crate::features::formatting::open_texts::match_open_text(uri, Some(&doc.text)) {
        crate::features::formatting::open_texts::OpenMatch::Current(version) if version == doc.version => {
            Some((doc.version, lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics)))
        }
        _ => None,
    }
}

/// Publish the diagnostics the analysis of an open document's newest text stored, at once.
///
/// An analysis judges the edited document's own unit in the project's database, with the rules
/// and the query gate a publication runs, so its diagnostics are the document's diagnostics; the
/// edited file's diagnostics no longer wait for the debounce and a second preparation of the
/// whole assembly (`publish_diagnostics_for_uri`, which follows and publishes again only when its
/// diagnostics differ). Nothing is published when a newer text was sent meanwhile: its own
/// analysis publishes.
pub async fn publish_analyzed_diagnostics(client: &Client, state: &RwLock<State>, uri: &Uri) {
    let update_gate = document_update_gate(state, uri).await;
    let _update_guard = update_gate.lock().await;
    let (version, diagnostics) = {
        let mut write = state.write().await;
        let Some((version, diagnostics)) = analyzed_diagnostics(&write, uri) else {
            return;
        };
        if !record_publication(&mut write, uri, version, &diagnostics) {
            return;
        }
        (version, diagnostics)
    };
    crate::trace::event("publish-analyzed", || {
        format!("uri={} version={version} count={}", uri.as_str(), diagnostics.len())
    });
    client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tokio::sync::RwLock;
    use tower_lsp_server::ls_types::Uri;

    use super::{analyzed_diagnostics, apply_if_current, record_publication};
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
            syntax_attribute_names: Vec::new(),
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

    /// The diagnostics an analysis stored are published only for the newest open text, and a
    /// later publication of the same diagnostics for that version sends nothing.
    #[test]
    fn analyzed_diagnostics_belong_to_the_newest_open_text_only() {
        use tower_lsp_server::ls_types::{
            DidChangeTextDocumentParams, DidOpenTextDocumentParams, TextDocumentContentChangeEvent, TextDocumentItem,
            VersionedTextDocumentIdentifier,
        };
        let uri = Uri::from_str("file:///tmp/beskid-lspfast-publish/Main.bd").expect("URI");
        let text = "i32 Main() { return 0; }";
        let mut state = State::default();
        let mut doc = empty_document(1, text);
        doc.syntax_diagnostics = vec![SyntaxDiagnostic {
            start: 0,
            end: 3,
            severity: SyntaxDiagnosticSeverity::Error,
            code: Some("E1201".into()),
            message: "analyzed".into(),
            source: "beskid".into(),
            detail: Default::default(),
        }];
        state.docs.insert(uri.clone(), doc);
        let sync = |method: &'static str, params: serde_json::Value| {
            let request = tower_lsp_server::jsonrpc::Request::build(method).params(params).finish();
            crate::features::formatting::open_texts::observe(&request);
        };
        sync(
            "textDocument/didOpen",
            serde_json::to_value(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "beskid".into(),
                    version: 1,
                    text: text.into(),
                },
            })
            .unwrap(),
        );
        let (version, diagnostics) = analyzed_diagnostics(&state, &uri).expect("the newest text's diagnostics");
        assert_eq!((version, diagnostics.len()), (1, 1));
        assert!(record_publication(&mut state, &uri, 1, &diagnostics), "first publication sends");
        assert!(!record_publication(&mut state, &uri, 1, &diagnostics), "the same diagnostics again send nothing");
        assert!(record_publication(&mut state, &uri, 1, &[]), "different diagnostics send");
        sync(
            "textDocument/didChange",
            serde_json::to_value(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri: uri.clone(), version: 2 },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: format!("{text}\n"),
                }],
            })
            .unwrap(),
        );
        assert!(analyzed_diagnostics(&state, &uri).is_none(), "facts of an older text are not published");
        sync("textDocument/didClose", serde_json::json!({"textDocument": {"uri": uri.as_str()}}));
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
            detail: Default::default(),
        };
        let old_generation = state.read().await.diagnostic_generation;
        invalidate_compilation_cache(&state).await;
        let mut write = state.write().await;
        assert!(!apply_if_current(&mut write, &uri, 2, text, old_generation, vec![old_diagnostic], Vec::new()));
        assert!(write.docs[&uri].syntax_diagnostics.is_empty());
    }
}
