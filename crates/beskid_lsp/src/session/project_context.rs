use std::path::PathBuf;

use tokio::sync::RwLock;

use super::db_access::with_compilation_db_mut_state;
use super::lifecycle::ANALYSIS_CACHE_VERSION;
use super::store::State;

fn project_graph_options_from_env() -> beskid_analysis::ProjectGraphBuildOptions {
    beskid_analysis::ProjectGraphBuildOptions {
        workspace_member_for_meta_default: std::env::var(
            "BESKID_WORKSPACE_MEMBER_FOR_META_DEFAULT",
        )
        .ok()
        .and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
    }
}

/// Returns a cached [`CompilationContext`] for `path`, or builds and inserts one keyed by the
/// resolved `.bproj` path.
pub async fn cached_compilation_context(
    state: &RwLock<State>,
    path: &std::path::Path,
) -> Option<beskid_analysis::CompilationContext> {
    let focused = { state.read().await.focused_project.clone() };
    let (manifest, _) = match beskid_analysis::resolve_project_manifest_for_source_path(path, None)
        .ok()
        .flatten()
    {
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
    let graph_options = project_graph_options_from_env();
    let cache_key = (
        manifest.clone(),
        graph_options.workspace_member_for_meta_default.clone(),
    );
    {
        let s = state.read().await;
        if let Some(ctx) = s.compilation_context_cache.get(&cache_key) {
            return Some(ctx.clone());
        }
    }
    let ctx = beskid_analysis::CompilationContext::try_for_analysis_path_with_graph_options(
        path,
        None,
        graph_options,
    )?;
    state
        .write()
        .await
        .compilation_context_cache
        .insert(cache_key, ctx.clone());
    Some(ctx)
}

/// Clear cached [`CompilationContext`] entries (e.g. after manifest or workspace graph changes).
pub async fn invalidate_compilation_cache(state: &RwLock<State>) {
    let (project_roots, focused_roots, cold_start) = {
        let read = state.read().await;
        let project_roots: Vec<PathBuf> = read
            .compilation_context_cache
            .keys()
            .filter_map(|(manifest, _)| manifest.parent().map(std::path::Path::to_path_buf))
            .collect();
        let mut focused_roots = Vec::new();
        if project_roots.is_empty()
            && let Some(focused) = read.focused_project.as_ref()
            && let Some(root) = focused.parent()
        {
            focused_roots.push(root.to_path_buf());
        }
        let cold_start =
            read.compilation_context_cache.is_empty() && read.configured_project_root.is_none();
        (project_roots, focused_roots, cold_start)
    };

    with_compilation_db_mut_state(state, |db, write| {
        for root in &project_roots {
            beskid_queries::invalidate_entry_sessions(root);
        }
        for root in &focused_roots {
            beskid_queries::invalidate_entry_sessions(root);
        }
        write.compilation_context_cache.clear();
        write.typed_prepare_schedule_revision.clear();
        if !cold_start {
            write.reset_compilation_db_with_db(db);
            for doc in write.docs.values_mut() {
                doc.analysis_cache_version = ANALYSIS_CACHE_VERSION.saturating_sub(1);
            }
        }
    })
    .await;
}
