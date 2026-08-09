use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::SourceUnit;
use crate::projects::{CompilePlan, PreparedProjectWorkspace};

/// Preserve the lexical origin of compiler-owned Foundation service units when a workspace
/// materializes them under `obj/beskid/deps`. A matching dependency name or source text is never
/// enough: the resolved dependency's original source root must contain the compiler-embedded
/// source path before its copied physical path is admitted.
pub(super) fn trusted_corelib_service_paths(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    units: &[SourceUnit],
) -> Arc<[PathBuf]> {
    let mut trusted = Vec::new();
    for logical_path in
        beskid_abi::runtime_source::canonical_corelib_service_sources().into_iter().map(|source| source.logical_path)
    {
        let Some(canonical_path) = beskid_abi::runtime_source::canonical_corelib_service_source_path(&logical_path)
        else {
            continue;
        };
        // Lexically clean both sides so `../..` from CARGO_MANIFEST_DIR matches a resolved
        // Foundation `source_root`. Do not canonicalize: symlink resolution would let a
        // user-project link to the compiler-owned file inherit panic/syscall provenance.
        let Some((index, dependency)) = plan.dependency_projects.iter().enumerate().find(|(_, dependency)| {
            let source_root = normalize_lexically(&dependency.source_root);
            canonical_path.starts_with(&source_root)
        }) else {
            continue;
        };
        let source_root = normalize_lexically(&dependency.source_root);
        let Ok(relative) = canonical_path.strip_prefix(&source_root) else {
            continue;
        };
        let effective_path = workspace
            .and_then(|workspace| workspace.materialized_dependencies.get(index))
            .map(|dependency| dependency.materialized_source_root.join(relative))
            .unwrap_or(canonical_path);
        if let Some(unit) = units.iter().find(|unit| paths_match(&unit.path, &effective_path)) {
            trusted.push(unit.path.clone());
        }
    }
    trusted.sort();
    trusted.dedup();
    Arc::from(trusted)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn paths_match(left: &Path, right: &Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}
