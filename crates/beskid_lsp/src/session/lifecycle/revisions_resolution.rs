use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

use beskid_analysis::services::{ResolvedInput, resolved_input_from_plan};
use tokio::sync::RwLock;

use crate::session::{project_context::cached_compilation_context, store::State};

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

pub(super) async fn resolved_input_for_path(
    state: &RwLock<State>,
    path: &Path,
    text: &str,
) -> Option<(ResolvedInput, beskid_analysis::CompilationContext)> {
    let session = cached_compilation_context(state, path).await?;
    let plan = session.compile_plan.clone()?;
    // The open buffer, not the build target's configured entry path or generated
    // object-root mirror, owns editor facts. The cached plan supplies dependencies
    // while this resolved input preserves the document's exact identity and text.
    let resolved = resolved_input_from_plan(path.to_path_buf(), text.to_string(), plan, None, None);
    Some((resolved, session))
}
