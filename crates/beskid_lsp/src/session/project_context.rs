use tokio::sync::RwLock;

use super::store::State;

/// Returns a cached [`CompilationContext`] for `path`, or builds and inserts one keyed by the
/// resolved `.bproj` path.
pub async fn cached_compilation_context(
    state: &RwLock<State>,
    path: &std::path::Path,
) -> Option<beskid_analysis::CompilationContext> {
    let focused = { state.read().await.focused_project.clone() };
    let (manifest, _) = match beskid_analysis::resolve_project_manifest_for_source_path(path, None).ok().flatten() {
        Some(resolved) => resolved,
        None => {
            let focused_manifest = focused.as_ref()?;
            let focus_root = focused_manifest.parent()?;
            if path.starts_with(focus_root) {
                (focused_manifest.clone(), None)
            } else {
                return None;
            }
        }
    };
    {
        let s = state.read().await;
        if let Some(ctx) = s.compilation_context_cache.get(&manifest) {
            return Some(ctx.clone());
        }
    }
    let ctx = beskid_analysis::CompilationContext::try_for_analysis_path(path, None)?;
    state.write().await.compilation_context_cache.insert(manifest, ctx.clone());
    Some(ctx)
}

/// Clear cached [`CompilationContext`] entries (e.g. after manifest or workspace graph changes).
pub async fn invalidate_compilation_cache(state: &RwLock<State>) {
    let mut write = state.write().await;
    for root in write.compilation_context_cache.keys().filter_map(|manifest| manifest.parent()) {
        beskid_queries::invalidate_entry_sessions(root);
    }
    if write.compilation_context_cache.is_empty()
        && let Some(root) = write.focused_project.as_ref().and_then(|focused| focused.parent())
    {
        beskid_queries::invalidate_entry_sessions(root);
    }
    let cold_start = write.compilation_context_cache.is_empty() && write.databases.is_empty();
    write.diagnostic_generation = write.diagnostic_generation.wrapping_add(1);
    write.compilation_context_cache.clear();
    if !cold_start {
        let dropped = write.databases.clear();
        super::db_access::drop_in_background(dropped);
        // Fail closed: drop bound facts until rebuild_open_document_syntax_facts runs.
        for doc in write.docs.values_mut() {
            doc.clear_syntax_facts();
        }
        for doc in write.workspace_index.values_mut() {
            doc.clear_syntax_facts();
        }
    }
}

/// Effective source roots (host first, then dependency packages) of the project that owns `path`.
pub async fn effective_roots_for_path(
    state: &RwLock<State>,
    path: &std::path::Path,
) -> Option<beskid_analysis::projects::EffectiveCompilationRoots> {
    let context = cached_compilation_context(state, path).await?;
    let plan = context.compile_plan.as_ref()?;
    let lock = plan.project_root.join("Project.lock");
    Some(
        beskid_analysis::projects::effective_roots_from_lockfile_checked(plan, &lock)
            .unwrap_or_else(|_| beskid_analysis::projects::effective_roots_from_plan_and_workspace(plan, None)),
    )
}
