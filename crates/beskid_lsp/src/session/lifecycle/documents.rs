use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use beskid_analysis::services::PrepareOptions;
use beskid_queries::typed_entry_state_with_db;

use crate::{
    manifest_uri::is_manifest_uri,
    session::{
        db_access::with_compilation_db_mut_state,
        diagnostics_bridge::collect_syntax_diagnostics_for_state,
        documentation_facts::syntax_documentation_facts_for_source,
        startup::wait_for_initial_scan,
        store::{Document, State},
    },
    workspace_scan::uri_to_path,
};

use super::{
    facts::{SyntaxFacts, syntax_facts_for_entry},
    revisions_resolution::{resolved_input_for_path, touch_entry_file_revision_for_uri},
};

pub(super) async fn build_syntax_facts(state: &RwLock<State>, uri: &Uri, text: &str) -> SyntaxFacts {
    wait_for_initial_scan(state).await;
    let documentation =
        if is_manifest_uri(uri) { Vec::new() } else { syntax_documentation_facts_for_source(uri.as_str(), text) };
    if is_manifest_uri(uri) {
        let diagnostics = collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        return SyntaxFacts { documentation, diagnostics, ..SyntaxFacts::default() };
    }
    let Some(path) = uri_to_path(uri) else {
        let diagnostics = collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        return SyntaxFacts { documentation, diagnostics, ..SyntaxFacts::default() };
    };
    let Some((resolved, session)) = resolved_input_for_path(state, &path, text).await else {
        let diagnostics = collect_syntax_diagnostics_for_state(state, uri, text, None).await;
        return SyntaxFacts { documentation, diagnostics, ..SyntaxFacts::default() };
    };
    let mut facts = with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = session.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        db.ensure_file_text(path, text.to_string());
        let options = PrepareOptions::default();
        match typed_entry_state_with_db(db, &resolved, &options, None) {
            Ok(entry_state) => syntax_facts_for_entry(db, &resolved, &entry_state),
            Err(_) => SyntaxFacts::default(),
        }
    })
    .await;
    facts.diagnostics = collect_syntax_diagnostics_for_state(state, uri, text, Some(&session)).await;
    facts.documentation = documentation;
    facts
}

fn document_from_syntax_facts(version: i32, text: String, syntax_facts: SyntaxFacts) -> Document {
    Document {
        version,
        text,
        syntax_definitions: syntax_facts.definitions,
        syntax_hovers: syntax_facts.hovers,
        syntax_symbols: syntax_facts.symbols,
        syntax_completion: syntax_facts.completion,
        syntax_inlay_hints: syntax_facts.inlay_hints,
        syntax_documentation: syntax_facts.documentation,
        syntax_diagnostics: syntax_facts.diagnostics,
    }
}

pub(super) fn apply_syntax_facts(doc: &mut Document, syntax_facts: SyntaxFacts) {
    doc.syntax_definitions = syntax_facts.definitions;
    doc.syntax_hovers = syntax_facts.hovers;
    doc.syntax_symbols = syntax_facts.symbols;
    doc.syntax_completion = syntax_facts.completion;
    doc.syntax_inlay_hints = syntax_facts.inlay_hints;
    doc.syntax_documentation = syntax_facts.documentation;
    doc.syntax_diagnostics = syntax_facts.diagnostics;
}

/// Build a [`Document`] for `uri` with generation-bound syntax facts for the buffer text.
pub async fn build_document(state: &RwLock<State>, uri: &Uri, version: i32, text: String) -> Document {
    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    document_from_syntax_facts(version, text, syntax_facts)
}

/// Store a disk-backed snapshot when the URI is not already an open buffer.
pub async fn set_disk_snapshot(state: &RwLock<State>, uri: Uri, doc: Document) {
    let mut write_state = state.write().await;
    if write_state.docs.contains_key(&uri) {
        return;
    }
    write_state.workspace_index.insert(uri, doc);
}

/// Upsert an open document, respecting monotonic versions.
///
/// Same-text updates still rebuild generation-bound syntax facts so hard invalidation cannot
/// leave a stale empty or orphaned fact set behind a text-hash fast path.
///
/// Returns `false` when `version` is stale relative to the buffered document (no mutation).
pub async fn set_document(state: &RwLock<State>, uri: Uri, version: i32, text: String) -> bool {
    {
        let mut write_state = state.write().await;
        write_state.workspace_index.remove(&uri);
        if let Some(existing) = write_state.docs.get(&uri)
            && version < existing.version
        {
            return false;
        }
    }

    touch_entry_file_revision_for_uri(state, &uri, &text).await;
    let syntax_facts = build_syntax_facts(state, &uri, &text).await;

    let mut write_state = state.write().await;
    if let Some(existing) = write_state.docs.get(&uri)
        && version < existing.version
    {
        return false;
    }
    write_state.docs.insert(uri, document_from_syntax_facts(version, text, syntax_facts));
    true
}

/// Drop an open buffer after `didClose` (disk hydration may repopulate the workspace index).
pub async fn remove_document(state: &RwLock<State>, uri: &Uri) {
    let mut write = state.write().await;
    write.docs.remove(uri);
    write.typed_prepare_schedule_revision.remove(uri);
}

/// Rebuild generation-bound syntax facts (including diagnostics) for open `.bd` buffers after
/// compilation context invalidation.
pub async fn rebuild_open_document_syntax_facts(state: &RwLock<State>) {
    let entries: Vec<(Uri, String)> = {
        let read = state.read().await;
        read.docs
            .iter()
            .filter(|(uri, _)| !is_manifest_uri(uri))
            .map(|(uri, doc)| (uri.clone(), doc.text.clone()))
            .collect()
    };

    for (uri, text) in entries {
        let syntax_facts = build_syntax_facts(state, &uri, &text).await;
        let mut write = state.write().await;
        if let Some(doc) = write.docs.get_mut(&uri)
            && doc.text == text
        {
            apply_syntax_facts(doc, syntax_facts);
        }
    }
}
