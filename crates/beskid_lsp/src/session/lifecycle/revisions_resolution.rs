use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

use beskid_analysis::services::{ResolvedInput, SessionFingerprint, resolve_input, resolved_input_from_plan};
use beskid_queries::{bump_file_revision, bump_typed_prepare_revision, fingerprint_key};
use tokio::sync::RwLock;
use tower_lsp_server::ls_types::Uri;

use crate::{
    session::{
        db_access::with_compilation_db_mut_state, project_context::cached_compilation_context,
        startup::wait_for_initial_scan, store::State,
    },
    workspace_scan::uri_to_path,
};

fn entry_key_for_resolved(resolved: &ResolvedInput) -> Option<String> {
    let plan = resolved.compile_plan.as_ref()?;
    Some(fingerprint_key(&SessionFingerprint::for_entry(plan, &resolved.source_path)))
}

pub(super) fn lockfile_digest_for_plan(plan: &beskid_analysis::projects::CompilePlan) -> String {
    let mut hasher = DefaultHasher::new();
    plan.project_root.hash(&mut hasher);
    plan.target.entry.hash(&mut hasher);
    plan.target.name.hash(&mut hasher);
    if let Ok(bytes) = std::fs::read(plan.project_root.join("Project.lock")) {
        bytes.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn bump_entry_file_revision(db: &mut beskid_queries::BeskidDatabase, resolved: &ResolvedInput) {
    if let Some(entry_key) = entry_key_for_resolved(resolved) {
        bump_file_revision(db, &entry_key);
    }
}

pub(super) fn bump_entry_typed_prepare_revision(db: &mut beskid_queries::BeskidDatabase, resolved: &ResolvedInput) {
    if let Some(entry_key) = entry_key_for_resolved(resolved) {
        bump_typed_prepare_revision(db, &entry_key);
    }
}

pub(super) async fn resolved_input_for_path(
    state: &RwLock<State>,
    path: &Path,
    text: &str,
) -> Option<(ResolvedInput, beskid_analysis::CompilationContext)> {
    let session = cached_compilation_context(state, path).await?;
    session.compile_plan.as_ref()?;
    let mut resolved = resolve_input(Some(&path.to_path_buf()), None, None, None, false, false).ok().or_else(|| {
        let plan = session.compile_plan.clone()?;
        Some(resolved_input_from_plan(path.to_path_buf(), text.to_string(), plan, None, None))
    })?;
    resolved.source = text.to_string();
    Some((resolved, session))
}

pub(super) async fn touch_entry_file_revision_for_uri(state: &RwLock<State>, uri: &Uri, text: &str) {
    wait_for_initial_scan(state).await;

    let Some(path) = uri_to_path(uri) else {
        return;
    };
    let Some((resolved, _)) = resolved_input_for_path(state, &path, text).await else {
        return;
    };
    with_compilation_db_mut_state(state, |db, write| {
        if let Some(plan) = resolved.compile_plan.as_ref() {
            write.configure_db_for_project_with_db(db, &plan.project_root);
        }
        db.ensure_file_text(path, text.to_string());
        bump_entry_file_revision(db, &resolved);
    })
    .await;
}
