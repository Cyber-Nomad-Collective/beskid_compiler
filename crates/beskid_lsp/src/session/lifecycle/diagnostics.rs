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
    let edits_at_start = crate::features::formatting::open_texts::edit_count();
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
        if let Some(published) = write.published_diagnostics.get(uri)
            && published.version == version
            && published.diagnostics != diagnostics
        {
            crate::trace::event("publish-corrects", || {
                correction_trace(uri, published, &diagnostics, generation, edits_at_start)
            });
        }
        // The analysis of this text published the same diagnostics already.
        if !record_publication(&mut write, uri, version, &diagnostics, None) {
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
    edits_at_analysis: Option<u64>,
) -> bool {
    if !state.docs.contains_key(uri) {
        return true;
    }
    if state
        .published_diagnostics
        .get(uri)
        .is_some_and(|published| published.version == version && published.diagnostics == diagnostics)
    {
        return false;
    }
    let generation = state.diagnostic_generation;
    state.published_diagnostics.insert(
        uri.clone(),
        crate::session::store::PublishedDiagnostics {
            version,
            diagnostics: diagnostics.to_vec(),
            generation,
            edits_at_analysis,
        },
    );
    true
}

/// What a correction of published diagnostics changed, and what changed in between: whether the
/// compilation context was invalidated, how many open-text changes (of other documents) came
/// between the analysis and the start of the full preparation, and up to three diagnostics on
/// each side.
fn correction_trace(
    uri: &Uri,
    published: &crate::session::store::PublishedDiagnostics,
    full: &[tower_lsp_server::ls_types::Diagnostic],
    full_generation: u64,
    full_edits: u64,
) -> String {
    let show = |diagnostic: &tower_lsp_server::ls_types::Diagnostic| {
        let code = match &diagnostic.code {
            Some(tower_lsp_server::ls_types::NumberOrString::String(code)) => code.clone(),
            Some(tower_lsp_server::ls_types::NumberOrString::Number(code)) => code.to_string(),
            None => "-".into(),
        };
        let message: String = diagnostic.message.chars().take(60).collect::<String>().replace(' ', "_");
        format!("{code}@{}:{message}", diagnostic.range.start.line)
    };
    let only = |a: &[tower_lsp_server::ls_types::Diagnostic], b: &[tower_lsp_server::ls_types::Diagnostic]| {
        a.iter().filter(|d| !b.contains(d)).take(3).map(show).collect::<Vec<_>>().join(",")
    };
    format!(
        "uri={} version={} from_analysis={} gen_changed={} other_edits={} early={} full={} only_early=[{}] only_full=[{}]",
        uri.as_str(),
        published.version,
        published.edits_at_analysis.is_some(),
        published.generation != full_generation,
        published.edits_at_analysis.map_or(-1, |edits| full_edits as i64 - edits as i64),
        published.diagnostics.len(),
        full.len(),
        only(&published.diagnostics, full),
        only(full, &published.diagnostics),
    )
}

/// What changed the inputs of an open document's analysis while it ran: the open texts of every
/// document, the files on disk (as the client reports them) and the compilation context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBaseline {
    edits: u64,
    disk_events: u64,
    generation: u64,
}

impl AnalysisBaseline {
    /// The baseline now, taken when an analysis starts.
    pub async fn now(state: &RwLock<State>) -> Self {
        let edits = crate::features::formatting::open_texts::edit_count();
        let disk_events = crate::session::store::disk_event_count();
        AnalysisBaseline { edits, disk_events, generation: state.read().await.diagnostic_generation }
    }

    /// Whether the files and the compilation context are as they were at the baseline: no file
    /// changed on disk and the context was not invalidated.
    fn same_files(&self, generation: u64) -> bool {
        crate::session::store::disk_event_count() == self.disk_events && generation == self.generation
    }

    /// Whether nothing an analysis reads changed since the baseline: no open text of any document
    /// (a cross-file effect on this document), no file on disk and no compilation context.
    pub async fn unchanged(&self, state: &RwLock<State>) -> bool {
        let generation = state.read().await.diagnostic_generation;
        crate::features::formatting::open_texts::edit_count() == self.edits && self.same_files(generation)
    }
}

/// The diagnostics the analysis of the newest text of an open document stored with its facts,
/// with that text's version: `None` when the stored facts are not those of the newest open text
/// (a newer edit is being analyzed, or the document was closed), or when that analysis did not
/// prepare its project (a structural fallback or a preparation failure: the full preparation
/// publishes those).
fn analyzed_diagnostics(state: &State, uri: &Uri) -> Option<(i32, Vec<tower_lsp_server::ls_types::Diagnostic>)> {
    let doc = state.docs.get(uri)?;
    if state.prepared_analyses.get(uri) != Some(&doc.version) {
        return None;
    }
    match crate::features::formatting::open_texts::match_open_text(uri, Some(&doc.text)) {
        crate::features::formatting::open_texts::OpenMatch::Current(version) if version == doc.version => {
            Some((doc.version, lsp_diagnostics_from_syntax(&doc.text, &doc.syntax_diagnostics)))
        }
        _ => None,
    }
}

/// Publish the diagnostics the analysis of an open document's newest text stored, at once;
/// `true` when the document's published diagnostics are now those of that analysis.
///
/// An analysis that prepared its project judges the edited document's own unit in the project's
/// database, with the rules and the query gate a publication runs, so its diagnostics are the
/// document's diagnostics; the edited file's diagnostics do not wait for the debounce and a
/// second preparation of the whole assembly. Nothing is published (`false`, the full preparation
/// publishes) when a newer text was sent meanwhile, when the analysis did not prepare its project
/// (a preparation failure names a file the full preparation may name differently), or when a
/// file changed on disk or the compilation context was invalidated since the analysis started.
pub async fn publish_analyzed_diagnostics(
    client: &Client,
    state: &RwLock<State>,
    uri: &Uri,
    baseline: AnalysisBaseline,
) -> bool {
    let update_gate = document_update_gate(state, uri).await;
    let _update_guard = update_gate.lock().await;
    let (version, diagnostics) = {
        let mut write = state.write().await;
        if !baseline.same_files(write.diagnostic_generation) {
            return false;
        }
        let Some((version, diagnostics)) = analyzed_diagnostics(&write, uri) else {
            return false;
        };
        if !record_publication(&mut write, uri, version, &diagnostics, Some(baseline.edits)) {
            return true;
        }
        (version, diagnostics)
    };
    crate::trace::event("publish-analyzed", || {
        format!("uri={} version={version} count={}", uri.as_str(), diagnostics.len())
    });
    client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;
    true
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
        assert!(analyzed_diagnostics(&state, &uri).is_none(), "an analysis that did not prepare its project");
        state.prepared_analyses.insert(uri.clone(), 1);
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
        assert!(record_publication(&mut state, &uri, 1, &diagnostics, Some(0)), "first publication sends");
        assert!(
            !record_publication(&mut state, &uri, 1, &diagnostics, Some(0)),
            "the same diagnostics again send nothing"
        );
        assert!(record_publication(&mut state, &uri, 1, &[], None), "different diagnostics send");
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

    /// The full preparation is skipped only when nothing an analysis reads changed while it ran:
    /// an edit of any open text, a disk event or an invalidated compilation context each keep it.
    #[tokio::test]
    async fn an_analysis_baseline_sees_every_input_change() {
        let state = RwLock::new(State::default());
        let observe = |method: &'static str, params: serde_json::Value| {
            let request = tower_lsp_server::jsonrpc::Request::build(method).params(params).finish();
            crate::features::formatting::open_texts::observe(&request);
        };
        let other = "file:///tmp/beskid-lspfast-baseline/Other.bd";
        let baseline = super::AnalysisBaseline::now(&state).await;
        observe(
            "textDocument/didOpen",
            serde_json::json!({"textDocument": {"uri": other, "languageId": "beskid", "version": 1, "text": "x"}}),
        );
        assert!(!baseline.unchanged(&state).await, "another open text changed");
        observe("textDocument/didClose", serde_json::json!({"textDocument": {"uri": other}}));

        let baseline = super::AnalysisBaseline::now(&state).await;
        crate::session::store::note_disk_event();
        assert!(!baseline.unchanged(&state).await, "a file changed on disk");

        let baseline = super::AnalysisBaseline::now(&state).await;
        invalidate_compilation_cache(&state).await;
        assert!(!baseline.unchanged(&state).await, "the compilation context was invalidated");
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
