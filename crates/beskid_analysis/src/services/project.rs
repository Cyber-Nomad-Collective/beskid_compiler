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
        CompilePlan, DefaultTargetCommand, PreparedProjectWorkspace, ProjectGraphBuildOptions, ProjectKind,
        ProjectWorkspacePlan, UnresolvedDependencyPolicy, WorkspacePrepareOptions, WorkspaceResolutionSummary,
        build_project_graph_with_options, compile_plan::compile_plan_from_graph,
        discover_project_manifest_from_input_or_cwd, discover_project_manifest_in_dir,
        discover_workspace_manifest_in_dir, is_project_manifest_path, is_workspace_manifest_path,
        load_manifest_from_path, prepare_project_workspace_plan_with_options, reject_legacy_manifest_path,
        resolve_workspace_candidate_with_summary, select_default_target, workspace_plan_from_graph,
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
    let selected = observe_phase_result(pipeline, RESOLVE_MANIFEST, || {
        selected_project_manifest(input, project, workspace_member)
    })?;
    let (manifest_path, workspace_summary) = match selected {
        Some((path, summary)) => (Some(path), summary),
        None => (None, None),
    };

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

/// The project manifest a command resolves: `--project`, a manifest given as input, or the manifest
/// discovered from the input or the working directory.
fn selected_project_manifest(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    workspace_member: Option<&str>,
) -> Result<Option<(PathBuf, Option<WorkspaceResolutionSummary>)>> {
    let explicit_manifest = project
        .map(|path| resolve_project_manifest_path(path))
        .or_else(|| input.and_then(|path| infer_manifest_from_input(path)));
    match explicit_manifest {
        Some(explicit) => {
            Ok(Some(resolve_workspace_candidate_with_summary(&explicit, input.map(|p| p.as_path()), workspace_member)?))
        }
        None => discover_project_manifest_from_input_or_cwd(input, workspace_member),
    }
}

/// The target `command` selects when no `--target` is given (see
/// [`crate::projects::select_default_target`]), or `None` when the command names a source file
/// (that file is the entry) or resolves no host project manifest. A manifest that cannot be found
/// or read is left to the resolver, which reports it; only an ambiguous or missing default target
/// fails here.
pub fn default_target_for_input(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    workspace_member: Option<&str>,
    command: DefaultTargetCommand,
) -> Result<Option<String>> {
    if input.is_some_and(|path| path.is_file() && infer_manifest_from_input(path).is_none()) {
        return Ok(None);
    }
    let Ok(Some((manifest_path, _))) = selected_project_manifest(input, project, workspace_member) else {
        return Ok(None);
    };
    let Ok(manifest) = load_manifest_from_path(&manifest_path) else { return Ok(None) };
    if manifest.project.kind != ProjectKind::Host || manifest.targets.is_empty() {
        return Ok(None);
    }
    select_default_target(&manifest.targets, command)
        .map(|target| Some(target.name.clone()))
        .map_err(|error| anyhow::anyhow!("{}: {error}", manifest_path.display()))
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
