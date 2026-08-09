use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::{
    session::{db_access::with_compilation_db_mut_state, startup::wait_for_initial_scan, store::State},
    workspace_scan::uri_to_path,
};

use super::{
    documents::{apply_syntax_facts, build_syntax_facts},
    revisions_resolution::{bump_entry_typed_prepare_revision, resolved_input_for_path},
};

/// Debounce window for typed executable prepare (coalesced with diagnostic publish).
const TYPED_PREPARE_DEBOUNCE_MS: u64 = 120;

async fn apply_typed_prepare_rebuild(state: &RwLock<State>, uri: &Uri) {
    wait_for_initial_scan(state).await;

    let text = {
        let read = state.read().await;
        read.docs
            .get(uri)
            .map(|doc| doc.text.clone())
            .or_else(|| read.workspace_index.get(uri).map(|doc| doc.text.clone()))
    };
    let Some(text) = text else {
        return;
    };

    let Some(path) = uri_to_path(uri) else {
        return;
    };
    let Some((resolved, _)) = resolved_input_for_path(state, &path, &text).await else {
        return;
    };

    with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = resolved.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        bump_entry_typed_prepare_revision(db, &resolved);
    })
    .await;

    let syntax_facts = build_syntax_facts(state, uri, &text).await;
    let mut write = state.write().await;
    if let Some(doc) = write.docs.get_mut(uri)
        && doc.text == text
    {
        apply_syntax_facts(doc, syntax_facts);
    } else if let Some(doc) = write.workspace_index.get_mut(uri)
        && doc.text == text
    {
        apply_syntax_facts(doc, syntax_facts);
    }
}

/// Schedule debounced typed executable prepare after buffer edits (120ms coalescing).
pub async fn schedule_typed_prepare_rebuild(state: Arc<RwLock<State>>, uri: Uri) {
    let rev = {
        let mut write = state.write().await;
        let next = write.typed_prepare_schedule_revision.get(&uri).copied().unwrap_or(0).saturating_add(1);
        write.typed_prepare_schedule_revision.insert(uri.clone(), next);
        next
    };

    let state_for_task = state.clone();
    let uri_for_task = uri.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(TYPED_PREPARE_DEBOUNCE_MS)).await;
        let should_run = {
            let read = state_for_task.read().await;
            read.typed_prepare_schedule_revision.get(&uri_for_task).copied() == Some(rev)
        };
        if should_run {
            apply_typed_prepare_rebuild(&state_for_task, &uri_for_task).await;
        }
    });
}
