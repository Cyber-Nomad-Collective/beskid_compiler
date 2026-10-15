use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{RESOLVE_GRAPH, RESOLVE_MANIFEST, WORKSPACE_GRAPH_CHANGED, WORKSPACE_MATERIALIZE},
};

use super::diagnostics_emit::project_error_diagnostic;
use crate::{
    analysis::diagnostics::MietteReportError,
    projects::{
        CompilePlan, PreparedProjectWorkspace, ProjectGraphBuildOptions, ProjectKind, ProjectWorkspacePlan,
        UnresolvedDependencyPolicy, WorkspacePrepareOptions, WorkspaceResolutionSummary,
        build_project_graph_with_options, compile_plan::compile_plan_from_graph,
        discover_project_manifest_from_input_or_cwd, discover_project_manifest_in_dir,
        discover_workspace_manifest_in_dir, is_project_manifest_path, is_workspace_manifest_path,
        prepare_project_workspace_plan_with_options, reject_legacy_manifest_path,
        resolve_workspace_candidate_with_summary, workspace_plan_from_graph,
    },
};

pub struct ResolvedProject {
    pub compile_plan: Option<CompilePlan>,
    pub prepared_workspace: Option<PreparedProjectWorkspace>,
    pub workspace_summary: Option<WorkspaceResolutionSummary>,
}

pub fn resolve_project(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    options: WorkspacePrepareOptions,
) -> Result<ResolvedProject> {
    resolve_project_with_policy(
        input,
        project,
        target,
        workspace_member,
        options,
        UnresolvedDependencyPolicy::Error,
        None,
    )
}

pub fn resolve_project_with_policy(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    options: WorkspacePrepareOptions,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ResolvedProject> {
    resolve_project_with_mode(
        input,
        project,
        target,
        workspace_member,
        options,
        unresolved_dependency_policy,
        pipeline,
        false,
    )
}

/// Resolve dependency operations for Template authoring roots without fabricating a compile plan.
/// Compilation callers retain target selection and the E1877 authoring-root prohibition.
pub fn resolve_project_dependencies_with_policy(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    options: WorkspacePrepareOptions,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ResolvedProject> {
    resolve_project_with_mode(
        input,
        project,
        target,
        workspace_member,
        options,
        unresolved_dependency_policy,
        pipeline,
        true,
    )
}

fn resolve_project_with_mode(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    options: WorkspacePrepareOptions,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
    pipeline: Option<&dyn PipelineObserver>,
    dependencies_only: bool,
) -> Result<ResolvedProject> {
    let mut workspace_summary: Option<WorkspaceResolutionSummary> = None;

    let manifest_path =
        observe_phase_result(pipeline, RESOLVE_MANIFEST, || -> Result<Option<PathBuf>, anyhow::Error> {
            let explicit_manifest = project
                .map(|path| resolve_project_manifest_path(path))
                .or_else(|| input.and_then(|path| infer_manifest_from_input(path)));
            let discovered_manifest = if explicit_manifest.is_none() {
                discover_project_manifest_from_input_or_cwd(input, workspace_member)?
            } else {
                None
            };

            if let Some(explicit) = explicit_manifest {
                let (path, summary) =
                    resolve_workspace_candidate_with_summary(&explicit, input.map(|p| p.as_path()), workspace_member)?;
                workspace_summary = summary;
                Ok(Some(path))
            } else if let Some((path, summary)) = discovered_manifest {
                workspace_summary = summary;
                Ok(Some(path))
            } else {
                Ok(None)
            }
        })?;

    let (compile_plan, prepared_workspace) = match &manifest_path {
        Some(manifest) => {
            let (compile_plan, plan) = observe_phase_result(pipeline, RESOLVE_GRAPH, || {
                let manifest_src = fs::read_to_string(manifest).unwrap_or_default();
                let graph_options = ProjectGraphBuildOptions {
                    workspace_member_for_meta_default: workspace_member.map(str::to_string),
                };
                build_project_graph_with_options(manifest, graph_options)
                    .and_then(|graph| {
                        if dependencies_only
                            && target.is_none()
                            && graph.root_manifest.project.kind == ProjectKind::Template
                        {
                            workspace_plan_from_graph(&graph, unresolved_dependency_policy).map(|plan| (None, plan))
                        } else {
                            let plan = compile_plan_from_graph(graph, target, unresolved_dependency_policy)?;
                            let workspace_plan = ProjectWorkspacePlan::from(&plan);
                            Ok((Some(plan), workspace_plan))
                        }
                    })
                    .map_err(|err| {
                        anyhow::Error::new(MietteReportError::new(project_error_diagnostic(
                            &manifest.display().to_string(),
                            &manifest_src,
                            &err,
                        )))
                    })
            })?;

            observe_phase(pipeline, WORKSPACE_GRAPH_CHANGED, || {});

            let workspace = observe_phase_result(pipeline, WORKSPACE_MATERIALIZE, || {
                let manifest_src = fs::read_to_string(&plan.manifest_path).unwrap_or_default();
                prepare_project_workspace_plan_with_options(&plan, options, pipeline).map_err(|err| {
                    anyhow::Error::new(MietteReportError::new(project_error_diagnostic(
                        &plan.manifest_path.display().to_string(),
                        &manifest_src,
                        &err,
                    )))
                })
            })?;

            (compile_plan, Some(workspace))
        }
        None => (None, None),
    };

    Ok(ResolvedProject { compile_plan, prepared_workspace, workspace_summary })
}

fn resolve_project_manifest_path(project: &Path) -> PathBuf {
    if project.is_dir() {
        if let Ok(Some(manifest)) = discover_project_manifest_in_dir(project) {
            return manifest;
        }
        if let Ok(Some(manifest)) = discover_workspace_manifest_in_dir(project) {
            return manifest;
        }
        project.join("project.bproj")
    } else {
        let _ = reject_legacy_manifest_path(project);
        project.to_path_buf()
    }
}

pub(super) fn infer_manifest_from_input(input: &Path) -> Option<PathBuf> {
    if is_project_manifest_path(input) || is_workspace_manifest_path(input) {
        return Some(input.to_path_buf());
    }

    if input.extension().and_then(|ext| ext.to_str()) == Some("proj") {
        let _ = reject_legacy_manifest_path(input);
    }

    None
}
