//! Compile plan construction from a manifest path.
//!
//! Entry points differ only in which knobs are fixed vs caller-controlled:
//!
//! - [`build_compile_plan`]: convenience for normal builds — forwards to
//!   [`build_compile_plan_with_policy`] with [`UnresolvedDependencyPolicy::Error`]
//!   and default graph build options.
//! - [`build_compile_plan_with_policy`]: same as the full builder, but fixes
//!   [`ProjectGraphBuildOptions::default`] (for example default `Meta` workspace attachment rules).
//!   Use this when you need a custom [`UnresolvedDependencyPolicy`] but not custom graph resolution.
//! - [`build_compile_plan_with_policy_and_graph`]: full control — supplies both unresolved-dependency
//!   handling and [`ProjectGraphBuildOptions`] passed into the project graph builder.

use std::fs;
use std::path::Path;

use crate::projects::error::ProjectError;
use crate::projects::graph::{
    ProjectGraphBuildOptions, UnresolvedDependencyKind, build_project_graph_with_options,
    collect_dependency_projects, collect_unresolved_dependencies,
};
use crate::projects::model::{
    CompilePlan, DependencySource, ProjectKind, ProjectManifest, Target, TargetKind,
    UnresolvedDependencyNote, UnresolvedDependencyPolicy,
};
use crate::projects::parser::parse_manifest;

pub fn load_manifest_from_path(path: &Path) -> Result<ProjectManifest, ProjectError> {
    let source = fs::read_to_string(path).map_err(|source| ProjectError::ReadManifest {
        path: path.to_path_buf(),
        source,
    })?;
    parse_manifest(&source)
}

/// Build a compile plan with strict unresolved-dependency policy and default graph options.
pub fn build_compile_plan(
    manifest_path: &Path,
    target_name: Option<&str>,
) -> Result<CompilePlan, ProjectError> {
    build_compile_plan_with_policy(
        manifest_path,
        target_name,
        UnresolvedDependencyPolicy::Error,
    )
}

/// Build a compile plan with caller-controlled unresolved-dependency policy and default graph options.
pub fn build_compile_plan_with_policy(
    manifest_path: &Path,
    target_name: Option<&str>,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
) -> Result<CompilePlan, ProjectError> {
    build_compile_plan_with_policy_and_graph(
        manifest_path,
        target_name,
        unresolved_dependency_policy,
        ProjectGraphBuildOptions::default(),
    )
}

/// Build a compile plan with caller-controlled policy and project-graph build options.
pub fn build_compile_plan_with_policy_and_graph(
    manifest_path: &Path,
    target_name: Option<&str>,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
    graph_options: ProjectGraphBuildOptions,
) -> Result<CompilePlan, ProjectError> {
    let graph = build_project_graph_with_options(manifest_path, graph_options)?;
    let dependency_projects = collect_dependency_projects(&graph);
    let unresolved_dependencies = collect_unresolved_dependencies(&graph)
        .into_iter()
        .map(|dependency| UnresolvedDependencyNote {
            dependency_name: dependency.dependency_name,
            source: match dependency.kind {
                UnresolvedDependencyKind::Git => DependencySource::Git,
                UnresolvedDependencyKind::Registry => DependencySource::Registry,
            },
            descriptor: dependency.descriptor,
        })
        .collect::<Vec<_>>();

    let unresolved_that_must_error = unresolved_dependencies
        .iter()
        .filter(|dependency| dependency.source != DependencySource::Registry)
        .cloned()
        .collect::<Vec<_>>();

    if unresolved_dependency_policy == UnresolvedDependencyPolicy::Error
        && !unresolved_that_must_error.is_empty()
    {
        let details = unresolved_that_must_error
            .iter()
            .map(|dependency| {
                format!(
                    "{}({:?}={})",
                    dependency.dependency_name, dependency.source, dependency.descriptor
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ProjectError::UnresolvedExternalDependencies(details));
    }

    let has_std_dependency = graph.has_std_dependency;
    let manifest = graph.root_manifest;
    let project_root = graph.root_project_root;
    let normalized_manifest_path = graph.root_manifest_path;

    if manifest.project.kind == ProjectKind::Meta {
        return Err(ProjectError::meta_contract(
            "E1820",
            "cannot build a compilation plan for `type = Meta` projects; select a host `Project.proj`",
        ));
    }

    let target = match target_name {
        Some(name) => manifest
            .targets
            .iter()
            .find(|target| target.name == name)
            .cloned()
            .ok_or_else(|| ProjectError::TargetNotFound(name.to_string()))?,
        None => pick_default_target(&manifest.targets)
            .cloned()
            .ok_or_else(|| {
                ProjectError::Validation("manifest must declare at least one target".to_string())
            })?,
    };

    Ok(CompilePlan {
        source_root: project_root.join(&manifest.project.root),
        project_root,
        manifest_path: normalized_manifest_path,
        project_name: manifest.project.name,
        target,
        dependency_projects,
        unresolved_dependencies,
        has_std_dependency,
    })
}

fn pick_default_target(targets: &[Target]) -> Option<&Target> {
    targets
        .iter()
        .find(|target| target.kind == TargetKind::App)
        .or_else(|| targets.first())
}
