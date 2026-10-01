//! Derive target-free lock ownership and dependency materialization from the project graph.

use crate::projects::{
    error::ProjectError,
    graph::{ProjectGraph, UnresolvedDependencyKind, collect_dependency_projects, collect_unresolved_dependencies},
    model::{
        DependencySource, ProjectKind, ProjectWorkspacePlan, UnresolvedDependencyNote, UnresolvedDependencyPolicy,
    },
};

pub(crate) fn workspace_plan_from_graph(
    graph: &ProjectGraph,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
) -> Result<ProjectWorkspacePlan, ProjectError> {
    let unresolved_dependencies = collect_unresolved_dependencies(graph)
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
        .collect::<Vec<_>>();
    if unresolved_dependency_policy == UnresolvedDependencyPolicy::Error && !unresolved_that_must_error.is_empty() {
        let details = unresolved_that_must_error
            .iter()
            .map(|dependency| {
                format!("{}({:?}={})", dependency.dependency_name, dependency.source, dependency.descriptor)
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ProjectError::UnresolvedExternalDependencies(details));
    }

    Ok(ProjectWorkspacePlan {
        project_root: graph.root_project_root.clone(),
        manifest_path: graph.root_manifest_path.clone(),
        project_name: graph.root_manifest.project.name.clone(),
        source_root: (graph.root_manifest.project.kind != ProjectKind::Template)
            .then(|| graph.root_project_root.join(&graph.root_manifest.project.root)),
        dependency_projects: collect_dependency_projects(graph),
        unresolved_dependencies,
    })
}
