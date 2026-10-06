use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use beskid_abi::{corelib_bundle::verified_corelib_bundle_root, runtime_kit::installed_corelib_root};
use beskid_pipeline::{
    PipelineObserver, observe_phase_result,
    phases::{
        WORKSPACE_MATERIALIZE_LOCAL, WORKSPACE_MATERIALIZE_LOCKFILE, WORKSPACE_MATERIALIZE_PATH_DEPS,
        WORKSPACE_MATERIALIZE_REGISTRY,
    },
    report_progress,
};

use super::{
    filesystem::{copy_directory_when_newer, materialized_dependency_id},
    lockfile::{
        PortableLockPath, PortableLockPathBaseKind, ProjectLockDependencyEntry, ProjectLockSource,
        WorkspacePrepareOptions, sync_project_lockfile,
    },
    registry::materialize_registry_dependency,
};
use crate::projects::{
    error::ProjectError,
    model::{
        CompilePlan, MaterializedDependencyProject, PreparedProjectWorkspace, ProjectWorkspacePlan,
        ResolvedDependencyProject,
    },
};

pub(super) fn portable_entry_for_dependency(
    plan: &ProjectWorkspacePlan,
    dependency: &ResolvedDependencyProject,
    verified_corelib_root: Option<&Path>,
) -> Result<ProjectLockDependencyEntry, ProjectError> {
    let verified_corelib_root = verified_corelib_root
        .and_then(|root| root.canonicalize().ok())
        .filter(|root| dependency.project_root.canonicalize().is_ok_and(|project| project.starts_with(root)));
    let (source, anchor) = match verified_corelib_root.as_deref() {
        Some(root) => (ProjectLockSource::Corelib, root),
        None => (ProjectLockSource::Path, plan.project_root.as_path()),
    };
    let project = relative_lock_path(anchor, &dependency.project_root, "project", source == ProjectLockSource::Path)?;
    let manifest = relative_lock_path(&dependency.project_root, &dependency.manifest_path, "manifest", false)?;
    let source_root = relative_lock_path(&dependency.project_root, &dependency.source_root, "source_root", false)?;
    let materialized_name = materialized_dependency_id(&dependency.dependency_name, source, &project)?;
    let materialized_root = format!("obj/beskid/deps/src/{materialized_name}");
    PortableLockPath::parse("materialized_root", &materialized_root, PortableLockPathBaseKind::MaterializedRoot)?
        .resolve(&plan.project_root)?;
    Ok(ProjectLockDependencyEntry {
        name: dependency.dependency_name.clone(),
        source,
        project,
        manifest,
        source_root,
        materialized_root,
        resolved_version: None,
        artifact_digest: None,
        registry: None,
    })
}

fn relative_lock_path(base: &Path, target: &Path, field: &str, external_project: bool) -> Result<String, ProjectError> {
    let base = base.canonicalize().map_err(|error| {
        ProjectError::Validation(format!("lockfile `{field}` base {} cannot be resolved: {error}", base.display()))
    })?;
    let target = target.canonicalize().map_err(|error| {
        ProjectError::Validation(format!("lockfile `{field}` target {} cannot be resolved: {error}", target.display()))
    })?;
    let base_parts = base.components().collect::<Vec<_>>();
    let target_parts = target.components().collect::<Vec<_>>();
    let common = base_parts.iter().zip(&target_parts).take_while(|(left, right)| left == right).count();
    if common == 0 || (!external_project && common != base_parts.len()) {
        return Err(ProjectError::Validation(format!("lockfile `{field}` escapes its declared base")));
    }
    let mut segments = vec!["..".to_string(); base_parts.len() - common];
    for part in &target_parts[common..] {
        let Component::Normal(name) = part else {
            return Err(ProjectError::Validation(format!("lockfile `{field}` has an invalid path component")));
        };
        segments.push(
            name.to_str()
                .ok_or_else(|| ProjectError::Validation(format!("lockfile `{field}` is not UTF-8")))?
                .to_string(),
        );
    }
    let value = if segments.is_empty() && field == "source_root" { ".".to_string() } else { segments.join("/") };
    let kind = if external_project {
        PortableLockPathBaseKind::ExternalProject
    } else {
        PortableLockPathBaseKind::ProjectDirectory
    };
    PortableLockPath::parse(field, &value, kind)?;
    Ok(value)
}

pub(crate) fn verified_installed_corelib_root() -> Option<PathBuf> {
    let installed = installed_corelib_root().ok()?.canonicalize().ok()?;
    let aggregate =
        if installed.join("beskid_corelib").is_dir() { installed.join("beskid_corelib") } else { installed.clone() };
    let verified = verified_corelib_bundle_root(&aggregate)?;
    (verified.starts_with(&installed) || installed.starts_with(&verified)).then_some(verified)
}

pub fn prepare_project_workspace(plan: &CompilePlan) -> Result<PreparedProjectWorkspace, ProjectError> {
    prepare_project_workspace_with_options(plan, WorkspacePrepareOptions::default(), None)
}

pub fn prepare_project_workspace_with_options(
    plan: &CompilePlan,
    options: WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedProjectWorkspace, ProjectError> {
    prepare_project_workspace_plan_with_options(&ProjectWorkspacePlan::from(plan), options, pipeline)
}

/// Materialize dependencies and enforce the ordinary lock policy without selecting a target.
pub fn prepare_project_workspace_plan_with_options(
    plan: &ProjectWorkspacePlan,
    options: WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedProjectWorkspace, ProjectError> {
    let deps_root = plan.project_root.join("obj").join("beskid").join("deps").join("src");
    let root_materialized_project = plan.project_root.join("obj").join("beskid").join("root");
    let policy = super::resolution::ResolutionPolicy {
        locked: options.locked || options.frozen,
        offline: options.offline || options.frozen,
        refresh: if options.refresh_lock {
            super::resolution::RefreshScope::All
        } else {
            super::resolution::RefreshScope::None
        },
    };
    let resolved = super::resolution::ResolveWorkspaceDependencies(plan, &policy, options.refresh_lock)?;
    let mut lock_entries = resolved.entries;
    let resolved_registry = resolved.registry;
    for registry in &resolved_registry {
        registry.lock_entry()?;
        registry.cache_artifact(&plan.project_root)?;
    }

    fs::create_dir_all(&deps_root)
        .map_err(|source| ProjectError::MaterializationCreateDir { path: deps_root.clone(), source })?;

    let source_segment = plan
        .source_root
        .as_ref()
        .and_then(|root| root.file_name())
        .map(|segment| segment.to_string_lossy().to_string())
        .unwrap_or_else(|| "Src".to_string());
    let materialized_source_root = root_materialized_project.join(&source_segment);
    if let Some(source_root) = &plan.source_root {
        observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_LOCAL, || {
            copy_directory_when_newer(source_root, &materialized_source_root)?;
            materialize_generated_sources(source_root, &materialized_source_root)?;
            report_progress(pipeline, WORKSPACE_MATERIALIZE_LOCAL, 1, 1, source_segment.clone());
            Ok(())
        })?;
    }

    let mut materialized_dependencies = Vec::with_capacity(plan.dependency_projects.len());

    let path_deps_total = plan.dependency_projects.len() as u64;
    observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_PATH_DEPS, || {
        for (index, dependency) in plan.dependency_projects.iter().enumerate() {
            let materialized_root = plan.project_root.join(&lock_entries[index].materialized_root);
            copy_directory_when_newer(&dependency.project_root, &materialized_root)?;

            report_progress(
                pipeline,
                WORKSPACE_MATERIALIZE_PATH_DEPS,
                index as u64 + 1,
                path_deps_total.max(1),
                dependency.dependency_name.clone(),
            );

            let source_relative = dependency
                .source_root
                .strip_prefix(&dependency.project_root)
                .unwrap_or_else(|_| std::path::Path::new(""));
            materialized_dependencies.push(MaterializedDependencyProject {
                dependency_name: dependency.dependency_name.clone(),
                manifest_path: dependency.manifest_path.clone(),
                project_name: dependency.project_name.clone(),
                materialized_project_root: materialized_root.clone(),
                materialized_source_root: materialized_root.join(source_relative),
            });
        }
        Ok::<(), ProjectError>(())
    })?;

    let registry_deps_total = resolved_registry.len() as u64;
    observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_REGISTRY, || {
        for (index, resolved) in resolved_registry.into_iter().enumerate() {
            let dependency_name = resolved.dependency_name.clone();
            let (lock_entry, materialized_dependency) = materialize_registry_dependency(resolved)?;
            lock_entries.push(lock_entry);
            materialized_dependencies.push(materialized_dependency);
            report_progress(
                pipeline,
                WORKSPACE_MATERIALIZE_REGISTRY,
                index as u64 + 1,
                registry_deps_total.max(1),
                dependency_name,
            );
        }
        Ok::<(), ProjectError>(())
    })?;

    lock_entries.sort_by(|left, right| left.name.cmp(&right.name));
    let lockfile_path = observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_LOCKFILE, || {
        sync_project_lockfile(plan, &lock_entries, options)
    })?;

    let verified_package_identities = crate::projects::VerifiedPackageIdentities::prepare(
        plan,
        &materialized_source_root,
        &materialized_dependencies,
        &lock_entries,
        &lockfile_path,
    )?;

    Ok(PreparedProjectWorkspace {
        verified_package_identities,
        lockfile_path,
        materialized_project_root: root_materialized_project,
        materialized_source_root,
        materialized_dependencies,
    })
}

/// Mirror the package's checked-in generated sources beside the materialized source root.
///
/// Generated units live in `.generated/` next to the source root (`<root>/../.generated`), the
/// location assembly discovery and module resolution read for every effective root. Dependencies
/// are materialized as whole project roots and so carry it already; the host copies only its
/// source root, so its `.generated` sibling is mirrored here, and a stale mirror is removed when
/// the package no longer has generated sources.
fn materialize_generated_sources(source_root: &Path, materialized_source_root: &Path) -> Result<(), ProjectError> {
    let (Some(source_parent), Some(materialized_parent)) = (source_root.parent(), materialized_source_root.parent())
    else {
        return Ok(());
    };
    let generated = source_parent.join(".generated");
    let materialized_generated = materialized_parent.join(".generated");
    if generated.is_dir() {
        return copy_directory_when_newer(&generated, &materialized_generated);
    }
    if materialized_generated.exists() {
        fs::remove_dir_all(&materialized_generated)
            .map_err(|source| ProjectError::MaterializationPrune { path: materialized_generated.clone(), source })?;
    }
    Ok(())
}

#[cfg(test)]
mod generated_materialization_tests {
    use super::materialize_generated_sources;
    use std::fs;

    #[test]
    fn host_generated_sources_mirror_beside_the_materialized_source_root_and_prune_when_gone() {
        let base = std::env::temp_dir().join(format!("beskid_generated_mirror_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let project = base.join("package");
        let source_root = project.join("src");
        let generated_file = project.join(".generated/Core/Text/Regex/Generated.g.bd");
        fs::create_dir_all(&source_root).expect("source root");
        fs::create_dir_all(generated_file.parent().expect("generated parent")).expect("generated directory");
        fs::write(&generated_file, "pub i64 ParseDigit() { return 1_i64; }").expect("generated source");
        let materialized_source_root = project.join("obj/beskid/root/src");
        fs::create_dir_all(&materialized_source_root).expect("materialized source root");

        materialize_generated_sources(&source_root, &materialized_source_root).expect("mirror generated sources");
        let mirrored = project.join("obj/beskid/root/.generated/Core/Text/Regex/Generated.g.bd");
        assert_eq!(
            fs::read_to_string(&mirrored).expect("mirrored generated source"),
            "pub i64 ParseDigit() { return 1_i64; }"
        );

        fs::remove_dir_all(project.join(".generated")).expect("drop generated sources");
        materialize_generated_sources(&source_root, &materialized_source_root).expect("prune generated mirror");
        assert!(!project.join("obj/beskid/root/.generated").exists(), "a stale generated mirror is removed");
        let _ = fs::remove_dir_all(&base);
    }
}
