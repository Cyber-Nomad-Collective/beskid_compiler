use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{RESOLVE_GRAPH, RESOLVE_MANIFEST, WORKSPACE_GRAPH_CHANGED, WORKSPACE_MATERIALIZE},
};

use crate::analysis::diagnostics::MietteReportError;
use crate::projects::{
    CompilePlan, PreparedProjectWorkspace, ProjectGraphBuildOptions, UnresolvedDependencyPolicy,
    WorkspacePrepareOptions, WorkspaceResolutionSummary, build_compile_plan_with_policy_and_graph,
    discover_project_manifest_from_input_or_cwd, discover_project_manifest_in_dir, discover_workspace_manifest_in_dir,
    is_project_manifest_path, is_workspace_manifest_path, prepare_project_workspace_with_options,
    reject_legacy_manifest_path, resolve_workspace_candidate_with_summary,
};

use super::diagnostics_emit::project_error_diagnostic;

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
    frozen: bool,
    locked: bool,
) -> Result<ResolvedProject> {
    resolve_project_with_policy(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        UnresolvedDependencyPolicy::Error,
        None,
    )
}

pub fn resolve_project_with_policy(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
    pipeline: Option<&dyn PipelineObserver>,
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
            let plan = observe_phase_result(pipeline, RESOLVE_GRAPH, || {
                let manifest_src = fs::read_to_string(manifest).unwrap_or_default();
                let graph_options = ProjectGraphBuildOptions {
                    workspace_member_for_meta_default: workspace_member.map(str::to_string),
                };
                build_compile_plan_with_policy_and_graph(manifest, target, unresolved_dependency_policy, graph_options)
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
                prepare_project_workspace_with_options(&plan, WorkspacePrepareOptions { frozen, locked }, pipeline)
                    .map_err(|err| {
                        anyhow::Error::new(MietteReportError::new(project_error_diagnostic(
                            &plan.manifest_path.display().to_string(),
                            &manifest_src,
                            &err,
                        )))
                    })
            })?;

            (Some(plan), Some(workspace))
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
