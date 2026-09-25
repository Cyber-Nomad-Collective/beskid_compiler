use beskid_abi::runtime_source::{corelib_service_source_identity, corelib_source_locations_match};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::{EffectiveCompilationRoots, SourceUnit};
use crate::projects::CompilePlan;

/// Preserve the lexical origin of compiler-owned Foundation service units when a workspace
/// materializes them under `obj/beskid/deps`. A matching dependency name or source text is never
/// enough: the resolved dependency's original source root must contain the compiler-embedded
/// source path before its copied physical path is admitted.
pub(super) fn trusted_corelib_service_paths(
    plan: &CompilePlan,
    roots: &EffectiveCompilationRoots,
    units: &[SourceUnit],
) -> Arc<[PathBuf]> {
    let mut trusted = Vec::new();
    for logical_path in beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .chain(std::iter::once(beskid_abi::runtime_source::canonical_corelib_deadline_source()))
        .map(|source| source.logical_path)
    {
        let Some(identity) = corelib_service_source_identity(&logical_path) else {
            continue;
        };
        // The graph already resolves dependency identity. Preserve that policy, accepting
        // ordinary/verbatim Windows drive spelling without resolving a new user alias here.
        let Some((index, relative)) = plan.dependency_projects.iter().enumerate().find_map(|(index, dependency)| {
            let source_root = normalize_lexically(&dependency.source_root);
            [&identity.declared_path, &identity.canonical_path].into_iter().find_map(|path| {
                path.ancestors()
                    .find(|ancestor| corelib_source_locations_match(ancestor, &source_root))
                    .and_then(|ancestor| path.strip_prefix(ancestor).ok())
                    .map(|relative| (index, relative.to_path_buf()))
            })
        }) else {
            continue;
        };
        // Assembly already validated and selected these roots, including Project.lock replay.
        // Use that exact destination so provenance cannot diverge from source discovery.
        let mut matching_roots = roots.dependencies.iter().filter(|root| {
            root.dependency_name.as_deref() == Some(plan.dependency_projects[index].dependency_name.as_str())
        });
        let Some(root) = matching_roots.next() else {
            continue;
        };
        if matching_roots.next().is_some() {
            continue;
        }
        let effective_path = root.source_root.join(relative);
        if units.iter().any(|unit| corelib_source_locations_match(&unit.origin_path, &effective_path)) {
            // Retain the issuer's destination, not a canonicalized requesting unit's path.
            trusted.push(effective_path);
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
