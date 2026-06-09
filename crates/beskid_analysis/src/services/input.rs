use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use beskid_pipeline::PipelineObserver;

use crate::projects::{
    CompilePlan, PreparedProjectWorkspace, ProgramAssembly, UnresolvedDependencyPolicy,
    WorkspaceResolutionSummary, plan_entry_path,
};

use super::project::{infer_manifest_from_input, resolve_project_with_policy};
use super::synthetic_plan::synthetic_compile_plan_for_source;

/// Source path and text plus optional workspace materialization outputs from [`resolve_input`].
pub struct ResolvedInput {
    pub source_path: PathBuf,
    pub source: String,
    pub compile_plan: Option<CompilePlan>,
    pub prepared_workspace: Option<PreparedProjectWorkspace>,
    pub workspace_summary: Option<WorkspaceResolutionSummary>,
    /// Populated only by `beskid_queries::prepare_compilation_with_db` (Salsa `program_assembly`
    /// enrich step). [`resolve_input`] and friends leave this `None`.
    pub assembly: Option<ProgramAssembly>,
}

impl ResolvedInput {
    /// Return a copy of this input with a cached [`ProgramAssembly`].
    pub fn with_assembly(&self, assembly: ProgramAssembly) -> Self {
        Self {
            source_path: self.source_path.clone(),
            source: self.source.clone(),
            compile_plan: self.compile_plan.clone(),
            prepared_workspace: self.prepared_workspace.clone(),
            workspace_summary: self.workspace_summary.clone(),
            assembly: Some(assembly),
        }
    }
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
    let input_is_manifest = input
        .map(|path| infer_manifest_from_input(path).is_some())
        .unwrap_or(false);
    let mut compile_plan = resolved_project.compile_plan;
    let prepared_workspace = resolved_project.prepared_workspace;
    let workspace_summary = resolved_project.workspace_summary;

    if compile_plan.is_none() {
        if let Some(input_path) = input {
            if !input_is_manifest && input_path.is_file() {
                compile_plan = Some(synthetic_compile_plan_for_source(input_path));
            }
        }
    }

    let source_path = match (
        input,
        input_is_manifest,
        compile_plan.as_ref(),
        prepared_workspace.as_ref(),
    ) {
        (Some(input), false, _, _) => input.clone(),
        (_, _, Some(plan), Some(workspace)) => {
            plan_entry_path(plan, &workspace.materialized_source_root)
        }
        (_, _, Some(plan), None) => plan_entry_path(plan, &plan.source_root),
        (_, _, None, _) => {
            return Err(anyhow::anyhow!(
                "no input file provided and no `.bproj` manifest discovered"
            ));
        }
    };

    let source = if source_path.is_file() {
        fs::read_to_string(&source_path)
            .with_context(|| format!("Failed to read file: {}", source_path.display()))?
    } else if compile_plan
        .as_ref()
        .is_some_and(|plan| plan.target.entry.as_deref().unwrap_or("").trim().is_empty())
    {
        String::new()
    } else {
        fs::read_to_string(&source_path)
            .with_context(|| format!("Failed to read file: {}", source_path.display()))?
    };

    Ok(ResolvedInput {
        source_path,
        source,
        compile_plan,
        prepared_workspace,
        workspace_summary,
        assembly: None,
    })
}
