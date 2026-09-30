use beskid_abi::corelib_bundle::verified_corelib_bundle_roots;
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
    let bundle_candidates =
        plan.dependency_projects.iter().map(|dependency| dependency.project_root.clone()).collect::<Vec<_>>();
    let verified_bundle_roots = verified_corelib_bundle_roots(&bundle_candidates);
    for source in beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .chain(std::iter::once(beskid_abi::runtime_source::canonical_corelib_deadline_source()))
    {
        let Some(identity) = corelib_service_source_identity(&source.logical_path) else {
            continue;
        };
        // The graph already resolves dependency identity. Preserve that policy, accepting
        // ordinary/verbatim Windows drive spelling without resolving a new user alias here.
        // Canonical service paths are not all equally deep: for example,
        // `Testing/Assert.bd` has two components while `Core/Bytes/Slice.bd`
        // has three. Derive the package source root from the trusted logical
        // path instead of assuming every service is three levels below it.
        let logical_relative = Path::new(&source.logical_path);
        let canonical_source_root = identity.canonical_path.ancestors().nth(logical_relative.components().count());
        let Some(canonical_source_root) = canonical_source_root else {
            continue;
        };
        let Some(canonical_relative) = identity.canonical_path.strip_prefix(canonical_source_root).ok() else {
            continue;
        };
        if canonical_relative != logical_relative {
            continue;
        }
        let Some((index, relative, bundled)) =
            plan.dependency_projects.iter().enumerate().find_map(|(index, dependency)| {
                let source_root = normalize_lexically(&dependency.source_root);
                let checkout_relative =
                    [&identity.declared_path, &identity.canonical_path].into_iter().find_map(|path| {
                        path.ancestors()
                            .find(|ancestor| corelib_source_locations_match(ancestor, &source_root))
                            .and_then(|ancestor| path.strip_prefix(ancestor).ok())
                            .map(|relative| (index, relative.to_path_buf()))
                    });
                checkout_relative.map(|(index, relative)| (index, relative, false)).or_else(|| {
                    bundled_corelib_source_root(
                        dependency,
                        verified_bundle_roots[index].as_deref(),
                        &identity.canonical_path,
                        canonical_source_root,
                    )
                    .map(|_| (index, canonical_relative.to_path_buf(), true))
                })
            })
        else {
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
        if units.iter().any(|unit| {
            corelib_source_locations_match(&unit.origin_path, &effective_path)
                && (!bundled
                    || (std::fs::read_to_string(&effective_path).is_ok_and(|contents| contents == source.source)
                        && unit.source == source.source
                        && is_regular_non_symlink_file(&unit.origin_path)
                        && unit
                            .origin_path
                            .canonicalize()
                            .is_ok_and(|physical| corelib_source_locations_match(&unit.path, &physical))))
        }) {
            // Retain the issuer's destination, not a canonicalized requesting unit's path.
            trusted.push(effective_path);
        }
    }
    trusted.sort();
    trusted.dedup();
    Arc::from(trusted)
}

fn bundled_corelib_source_root(
    dependency: &crate::projects::ResolvedDependencyProject,
    bundle_root: Option<&Path>,
    canonical_service_path: &Path,
    canonical_source_root: &Path,
) -> Option<PathBuf> {
    let bundle_root = bundle_root?;
    let canonical_package_root = canonical_source_root.parent()?;
    let canonical_packages_root = canonical_package_root.parent()?;
    if canonical_packages_root.file_name().is_none_or(|name| name != "packages") {
        return None;
    }
    let canonical_workspace_root = canonical_packages_root.parent()?;
    let expected_project_relative = canonical_package_root.strip_prefix(canonical_workspace_root).ok()?;

    let physical_project_root = dependency.project_root.canonicalize().ok()?;
    if physical_project_root.strip_prefix(bundle_root).ok()? != expected_project_relative {
        return None;
    }
    let physical_source_root = dependency.source_root.canonicalize().ok()?;
    if physical_source_root.strip_prefix(&physical_project_root).ok()? != Path::new("src") {
        return None;
    }
    let relative = canonical_service_path.strip_prefix(canonical_source_root).ok()?;
    let installed_source = physical_source_root.join(relative);
    if !is_regular_non_symlink_file(&installed_source)
        || std::fs::read_to_string(&installed_source).ok()? != std::fs::read_to_string(canonical_service_path).ok()?
    {
        return None;
    }
    Some(physical_source_root)
}

fn is_regular_non_symlink_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
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
