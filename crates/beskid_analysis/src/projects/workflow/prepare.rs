use std::{
    collections::HashSet,
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
        WorkspacePrepareOptions, preflight_existing_lock_for_plan, sync_project_lockfile, validate_existing_lock_graph,
    },
    registry::{materialize_registry_dependency, resolve_registry_dependency},
};
use crate::projects::{
    error::ProjectError,
    graph::builder::discover_workspace_resolution_rules,
    model::{
        CompilePlan, DependencySource, MaterializedDependencyProject, PreparedProjectWorkspace, ProjectWorkspacePlan,
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
    let verified_corelib_root = verified_installed_corelib_root();
    let existing_lock = preflight_existing_lock_for_plan(plan, options)?;
    // Refresh reconstructs registry identity from the current manifest graph.
    let existing_registry_pins = if options.refresh_lock {
        None
    } else {
        existing_lock.as_ref().map(|lock| {
            lock.dependencies
                .iter()
                .filter(|entry| entry.source == ProjectLockSource::Registry)
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    let mut lock_entries = Vec::with_capacity(plan.dependency_projects.len());
    let mut destinations = HashSet::new();
    for dependency in &plan.dependency_projects {
        let entry = portable_entry_for_dependency(plan, dependency, verified_corelib_root.as_deref())?;
        if !destinations.insert(entry.materialized_root.clone()) {
            return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
        }
        lock_entries.push(entry);
    }

    let workspace_rules = discover_workspace_resolution_rules(&plan.manifest_path)?;
    let registry_deps: Vec<_> =
        plan.unresolved_dependencies.iter().filter(|x| x.source == DependencySource::Registry).collect();
    validate_existing_lock_graph(
        existing_lock.as_ref(),
        &lock_entries,
        &registry_deps.iter().map(|entry| entry.dependency_name.as_str()).collect::<Vec<_>>(),
        options.refresh_lock,
    )?;
    let mut resolved_registry = Vec::with_capacity(registry_deps.len());
    for unresolved in &registry_deps {
        let pinned = existing_registry_pins
            .as_ref()
            .and_then(|entries| entries.iter().find(|entry| entry.name == unresolved.dependency_name));
        let missing_existing_pin = existing_registry_pins.is_some() && pinned.is_none() && !options.refresh_lock;
        if missing_existing_pin && (options.locked || options.frozen) {
            return Err(ProjectError::Validation(format!(
                "registry dependency `{}` is missing a pin in the existing v2 lock; run `beskid update`",
                unresolved.dependency_name
            )));
        }
        let Some(resolved) = resolve_registry_dependency(
            unresolved,
            workspace_rules.as_ref(),
            pinned,
            options.refresh_lock,
            &plan.project_root,
        )?
        else {
            continue;
        };
        if missing_existing_pin {
            return Err(ProjectError::Validation(format!(
                "registry dependency `{}` became available but is missing a pin in the existing v2 lock; run `beskid \
                 update`",
                unresolved.dependency_name
            )));
        }
        resolved.validate_existing_lock_entry(pinned)?;
        if !destinations.insert(resolved.materialized_relative.clone()) {
            return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
        }
        resolved_registry.push(resolved);
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

    Ok(PreparedProjectWorkspace {
        lockfile_path,
        materialized_project_root: root_materialized_project,
        materialized_source_root,
        materialized_dependencies,
    })
}
