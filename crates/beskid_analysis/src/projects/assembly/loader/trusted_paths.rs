use beskid_abi::corelib_bundle::verified_corelib_bundle_roots;
use beskid_abi::runtime_source::{
    CorelibServiceSourceDescriptor, corelib_service_source_descriptor, corelib_service_source_identity,
    corelib_source_locations_match,
};
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
        let Some(descriptor) = corelib_service_source_descriptor(&source.logical_path) else {
            continue;
        };
        let logical_relative = Path::new(descriptor.logical_path());
        if descriptor.relative_path() != logical_relative || descriptor.canonical_source() != source.source {
            continue;
        }
        // Physical build-checkout identity is optional. It remains the authority for direct
        // source/development dependencies, but verified installed bundles use only the immutable
        // logical descriptor above and therefore survive compiler relocation.
        let checkout_identity = corelib_service_source_identity(&source.logical_path);
        let Some((index, relative, bundled)) =
            plan.dependency_projects.iter().enumerate().find_map(|(index, dependency)| {
                let source_root = normalize_lexically(&dependency.source_root);
                let checkout_relative = checkout_identity.as_ref().and_then(|identity| {
                    [&identity.declared_path, &identity.canonical_path].into_iter().find_map(|path| {
                        path.ancestors()
                            .find(|ancestor| corelib_source_locations_match(ancestor, &source_root))
                            .and_then(|ancestor| path.strip_prefix(ancestor).ok())
                            .map(|relative| (index, relative.to_path_buf()))
                    })
                });
                checkout_relative.map(|(index, relative)| (index, relative, false)).or_else(|| {
                    bundled_corelib_source_root(dependency, verified_bundle_roots[index].as_deref(), &descriptor)
                        .map(|_| (index, descriptor.relative_path().to_path_buf(), true))
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
    descriptor: &CorelibServiceSourceDescriptor,
) -> Option<PathBuf> {
    let bundle_root = bundle_root?;
    let expected_project_relative = Path::new("packages").join(descriptor.package());

    let physical_project_root = dependency.project_root.canonicalize().ok()?;
    if physical_project_root.strip_prefix(bundle_root).ok()? != expected_project_relative {
        return None;
    }
    let physical_source_root = dependency.source_root.canonicalize().ok()?;
    if physical_source_root.strip_prefix(&physical_project_root).ok()? != Path::new("src") {
        return None;
    }
    let installed_source = physical_source_root.join(descriptor.relative_path());
    if !is_regular_non_symlink_file(&installed_source)
        || std::fs::read_to_string(&installed_source).ok()? != descriptor.canonical_source()
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
