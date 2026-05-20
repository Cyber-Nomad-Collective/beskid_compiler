use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use beskid_pipeline::PipelineObserver;

use crate::projects::{
    AssemblyDiscovery, AssemblyOptions, CompilePlan, PROJECT_FILE_NAME, PreparedProjectWorkspace,
    ProgramAssembly, UnresolvedDependencyPolicy, WorkspaceResolutionSummary, assemble_program,
};

use super::project::{infer_manifest_from_input, resolve_project_with_policy};

/// Source path and text plus optional workspace materialization outputs from [`resolve_input`].
pub struct ResolvedInput {
    pub source_path: PathBuf,
    pub source: String,
    pub compile_plan: Option<CompilePlan>,
    pub prepared_workspace: Option<PreparedProjectWorkspace>,
    pub workspace_summary: Option<WorkspaceResolutionSummary>,
    pub assembly: Option<ProgramAssembly>,
}

/// Optional workspace member name for analysis parity with CLI `--workspace-member`.
#[derive(Clone, Default)]
pub struct AnalyzeInProjectOptions<'a> {
    pub workspace_member: Option<&'a str>,
    /// Forwarded into workspace graph construction (for example `workspace_member_for_meta_default`
    /// for `attachTo: default` when multiple members exist).
    pub project_graph: crate::projects::ProjectGraphBuildOptions,
}

pub fn resolve_input(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
) -> Result<ResolvedInput> {
    resolve_input_with_policy(
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

/// Like [`resolve_input`] but forwards an optional [`PipelineObserver`] for workspace resolve / graph / materialize.
pub fn resolve_input_with_pipeline(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ResolvedInput> {
    resolve_input_with_policy(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        UnresolvedDependencyPolicy::Error,
        pipeline,
    )
}

pub fn resolve_input_with_policy(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ResolvedInput> {
    let resolved_project = resolve_project_with_policy(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        unresolved_dependency_policy,
        pipeline,
    )?;
    let compile_plan = resolved_project.compile_plan;
    let prepared_workspace = resolved_project.prepared_workspace;
    let workspace_summary = resolved_project.workspace_summary;
    let input_is_manifest = input
        .map(|path| infer_manifest_from_input(path).is_some())
        .unwrap_or(false);

    let source_path = match (
        input,
        input_is_manifest,
        compile_plan.as_ref(),
        prepared_workspace.as_ref(),
    ) {
        (Some(input), false, _, _) => input.clone(),
        (_, _, Some(plan), Some(workspace)) => {
            workspace.materialized_source_root.join(&plan.target.entry)
        }
        (_, _, Some(plan), None) => plan.source_root.join(&plan.target.entry),
        (_, _, None, _) => {
            return Err(anyhow::anyhow!(
                "no input file provided and no `{}` discovered",
                PROJECT_FILE_NAME
            ));
        }
    };

    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("Failed to read file: {}", source_path.display()))?;

    let mut assembly_options = AssemblyOptions::default();
    assembly_options.discovery = AssemblyDiscovery::ImportClosure;
    let assembly = compile_plan.as_ref().and_then(|plan| {
        assemble_program(
            plan,
            prepared_workspace.as_ref(),
            &source_path,
            Some(&source),
            &assembly_options,
        )
        .ok()
    });
    // Assembly is best-effort at resolve time; lowering will re-assemble if this failed.

    Ok(ResolvedInput {
        source_path,
        source,
        compile_plan,
        prepared_workspace,
        workspace_summary,
        assembly,
    })
}
