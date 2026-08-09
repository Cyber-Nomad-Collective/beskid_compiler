use std::fs;

use beskid_pipeline::{
    PipelineObserver, observe_phase_result,
    phases::{
        WORKSPACE_MATERIALIZE_LOCAL, WORKSPACE_MATERIALIZE_LOCKFILE, WORKSPACE_MATERIALIZE_PATH_DEPS,
        WORKSPACE_MATERIALIZE_REGISTRY,
    },
    report_progress,
};

use super::filesystem::{copy_directory_when_newer, materialized_dependency_id};
use super::lockfile::{ProjectLockDependencyEntry, WorkspacePrepareOptions, sync_project_lockfile};
use super::registry::materialize_registry_dependency;
use crate::projects::error::ProjectError;
use crate::projects::graph::builder::discover_workspace_resolution_rules;
use crate::projects::model::{CompilePlan, DependencySource, MaterializedDependencyProject, PreparedProjectWorkspace};

pub fn prepare_project_workspace(plan: &CompilePlan) -> Result<PreparedProjectWorkspace, ProjectError> {
    prepare_project_workspace_with_options(plan, WorkspacePrepareOptions::default(), None)
}

pub fn prepare_project_workspace_with_options(
    plan: &CompilePlan,
    options: WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedProjectWorkspace, ProjectError> {
    let deps_root = plan.project_root.join("obj").join("beskid").join("deps").join("src");
    let root_materialized_project = plan.project_root.join("obj").join("beskid").join("root");
    fs::create_dir_all(&deps_root)
        .map_err(|source| ProjectError::MaterializationCreateDir { path: deps_root.clone(), source })?;

    let workspace_rules = discover_workspace_resolution_rules(&plan.manifest_path)?;

    let source_segment = plan
        .source_root
        .file_name()
        .map(|segment| segment.to_string_lossy().to_string())
        .unwrap_or_else(|| "Src".to_string());
    let materialized_source_root = root_materialized_project.join(&source_segment);
    observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_LOCAL, || {
        copy_directory_when_newer(&plan.source_root, &materialized_source_root)?;
        report_progress(pipeline, WORKSPACE_MATERIALIZE_LOCAL, 1, 1, source_segment.clone());
        Ok(())
    })?;

    let mut lock_entries = Vec::with_capacity(plan.dependency_projects.len());
    let mut materialized_dependencies = Vec::with_capacity(plan.dependency_projects.len());

    let path_deps_total = plan.dependency_projects.len() as u64;
    observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_PATH_DEPS, || {
        for (index, dependency) in plan.dependency_projects.iter().enumerate() {
            let materialized_root =
                deps_root.join(materialized_dependency_id(&dependency.project_name, &dependency.manifest_path));
            copy_directory_when_newer(&dependency.project_root, &materialized_root)?;

            report_progress(
                pipeline,
                WORKSPACE_MATERIALIZE_PATH_DEPS,
                index as u64 + 1,
                path_deps_total.max(1),
                dependency.dependency_name.clone(),
            );

            lock_entries.push(ProjectLockDependencyEntry {
                name: dependency.dependency_name.clone(),
                manifest: dependency.manifest_path.display().to_string(),
                project: dependency.project_root.display().to_string(),
                source_root: dependency.source_root.display().to_string(),
                materialized_root: materialized_root.display().to_string(),
                resolved_version: None,
                artifact_digest: None,
                registry: None,
            });

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

    let registry_deps: Vec<_> =
        plan.unresolved_dependencies.iter().filter(|x| x.source == DependencySource::Registry).collect();
    let registry_deps_total = registry_deps.len() as u64;
    observe_phase_result(pipeline, WORKSPACE_MATERIALIZE_REGISTRY, || {
        for (index, unresolved) in registry_deps.iter().enumerate() {
            if let Some((lock_entry, materialized_dependency)) =
                materialize_registry_dependency(unresolved, &deps_root, workspace_rules.as_ref())?
            {
                lock_entries.push(lock_entry);
                materialized_dependencies.push(materialized_dependency);
                report_progress(
                    pipeline,
                    WORKSPACE_MATERIALIZE_REGISTRY,
                    index as u64 + 1,
                    registry_deps_total.max(1),
                    unresolved.dependency_name.clone(),
                );
            }
        }
        Ok::<(), ProjectError>(())
    })?;

    lock_entries.sort_by_key(|entry| entry.to_v1_line());
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
