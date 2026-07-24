use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use beskid_pipeline::PipelineObserver;

use crate::projects::{
    CompilePlan, PreparedProjectWorkspace, ProgramAssembly, UnresolvedDependencyPolicy, WorkspaceResolutionSummary,
    plan_entry_path,
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
    let input_is_manifest = input.map(|path| infer_manifest_from_input(path).is_some()).unwrap_or(false);
    let mut compile_plan = resolved_project.compile_plan;
    let prepared_workspace = resolved_project.prepared_workspace;
    let workspace_summary = resolved_project.workspace_summary;

    if compile_plan.is_none()
        && let Some(input_path) = input
        && !input_is_manifest
        && input_path.is_file()
    {
        compile_plan = Some(synthetic_compile_plan_for_source(input_path));
    }

    let source_path = if let Some(input_path) = input
        && !input_is_manifest
        && input_path.is_file()
    {
        materialized_input_path(input_path, compile_plan.as_ref(), prepared_workspace.as_ref())
    } else if let Some(plan) = compile_plan.as_ref() {
        let root = prepared_workspace
            .as_ref()
            .map(|workspace| workspace.materialized_source_root.as_path())
            .unwrap_or(plan.source_root.as_path());
        plan_entry_path(plan, root)
    } else if let Some(input_path) = input {
        if input_is_manifest || !input_path.is_file() {
            return Err(anyhow::anyhow!("no input file provided and no `.bproj` manifest discovered"));
        }
        input_path.clone()
    } else {
        return Err(anyhow::anyhow!("no input file provided and no `.bproj` manifest discovered"));
    };

    let source = if source_path.is_file() {
        fs::read_to_string(&source_path).with_context(|| format!("Failed to read file: {}", source_path.display()))?
    } else if compile_plan.as_ref().is_some_and(|plan| plan.target.entry.as_deref().unwrap_or("").trim().is_empty()) {
        String::new()
    } else {
        fs::read_to_string(&source_path).with_context(|| format!("Failed to read file: {}", source_path.display()))?
    };

    Ok(ResolvedInput { source_path, source, compile_plan, prepared_workspace, workspace_summary, assembly: None })
}

/// Preserve an explicit source-file selection when project resolution has a default manifest
/// entry. If a workspace materialized its sources, map that selected file into the materialized
/// root so downstream assembly and session fingerprints identify the same physical revision.
fn materialized_input_path(
    input_path: &std::path::Path,
    plan: Option<&CompilePlan>,
    workspace: Option<&PreparedProjectWorkspace>,
) -> PathBuf {
    let Some((plan, workspace)) = plan.zip(workspace) else {
        return input_path.to_path_buf();
    };
    let canonical_input = input_path.canonicalize().unwrap_or_else(|_| input_path.to_path_buf());
    let canonical_source_root = plan.source_root.canonicalize().unwrap_or_else(|_| plan.source_root.clone());
    canonical_input
        .strip_prefix(canonical_source_root)
        .map(|relative| workspace.materialized_source_root.join(relative))
        .unwrap_or(canonical_input)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::resolve_input;

    fn compiler_workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("compiler workspace root")
            .to_path_buf()
    }

    #[test]
    fn resolve_input_directory_with_compile_plan_uses_entry_file() {
        let root = compiler_workspace_root();
        let workspace_root = root.join("corelib");
        let manifest = workspace_root.join("CoreLib.bws");
        if !manifest.is_file() {
            eprintln!("skip resolve_input_directory_with_compile_plan_uses_entry_file: {manifest:?} missing");
            return;
        }
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&workspace_root).expect("chdir");
        let resolved =
            resolve_input(Some(&workspace_root), None, None, None, false, false).expect("resolve directory input");
        std::env::set_current_dir(previous).expect("restore cwd");
        assert!(resolved.source_path.is_file(), "expected entry file, got {}", resolved.source_path.display());
        assert!(resolved.compile_plan.is_some());
    }
}
