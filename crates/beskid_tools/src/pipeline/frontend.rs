//! Thin wrappers around `beskid_analysis::services` for CLI resolution, parsing, and analysis gates.

use std::path::{Path, PathBuf};

use anyhow::Result;
use beskid_analysis::{
    SemanticDiagnostic,
    projects::{UnresolvedDependencyPolicy, WorkspacePrepareOptions},
    services::{self, ResolvedProject},
    syntax::{Program, Spanned},
};
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::SEMANTIC};

use super::{
    CliPipeline,
    resolve_options::{CliResolveOptions, FrontendProjectPipelineOptions},
};

/// Resolve `input` / `project` / lockfile flags the same way as most CLI subcommands.
pub fn resolve_input(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    options: WorkspacePrepareOptions,
) -> Result<services::ResolvedInput> {
    services::resolve_input(input, project, target, workspace_member, options)
}

/// Like [`resolve_input`], forwarding [`PipelineObserver`] events (e.g. for CLI progress).
pub fn resolve_input_with_pipeline(
    resolve: CliResolveOptions<'_>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<services::ResolvedInput> {
    services::resolve_input_with_policy(
        resolve.input,
        resolve.project,
        resolve.target,
        resolve.workspace_member,
        resolve.WorkspaceOptions(),
        UnresolvedDependencyPolicy::Error,
        pipeline,
    )
}

/// Resolve dependency operations, including target-free Template authoring roots.
pub fn resolve_project_with_pipeline(
    options: FrontendProjectPipelineOptions<'_>,
) -> Result<ResolvedProject> {
    let FrontendProjectPipelineOptions {
        resolve,
        unresolved_dependency_policy,
        pipeline,
    } = options;
    services::resolve_project_dependencies_with_policy(
        resolve.input,
        resolve.project,
        resolve.target,
        resolve.workspace_member,
        resolve.WorkspaceOptions(),
        unresolved_dependency_policy,
        pipeline,
    )
}

/// Parse `source` as a Beskid program using `path` for error labels.
pub fn parse_program(path: &Path, source: &str) -> Result<Spanned<Program>> {
    services::parse_program_with_source_name(&path.display().to_string(), source)
}

/// Fail if `source` does not parse as a Beskid program at `path`.
pub fn validate_source(path: &Path, source: &str) -> Result<()> {
    let _ = parse_program(path, source)?;
    Ok(())
}

/// Run semantic analysis, print diagnostics through the CLI session, and fail on errors.
pub fn run_semantic_analysis_gate(
    path: &Path,
    source: &str,
    pipeline: Option<&dyn PipelineObserver>,
    session: &CliPipeline,
) -> Result<()> {
    observe_phase_result(pipeline, SEMANTIC, || {
        let diagnostics = if let Some(plan) = services::compile_plan_for_input_path(path) {
            let resolved = services::resolved_input_from_plan(
                path.to_path_buf(),
                source.to_string(),
                plan,
                None,
                None,
            );
            let (_, diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics(
                &resolved,
                services::PrepareOptions {
                    mod_host: beskid_analysis::mod_host::ModHost::NoMods,
                    front_end: services::FrontEndOptions {
                        with_semantic_diagnostics: true,
                        ..Default::default()
                    },
                    native_mod_adapter_sources: false,
                },
                pipeline,
            )?;
            diagnostics
        } else {
            standalone_semantic_diagnostics(path, source, pipeline)?
        };
        session.report_semantic_diagnostics(&diagnostics);
        services::require_no_semantic_errors(&diagnostics).map_err(anyhow::Error::from)
    })
}

/// Diagnostics of a standalone `.bd` file that belongs to no project: the same query gate judges
/// it as a project entry, over a synthetic single-file `App` plan with no dependencies and no
/// Mods. A reserved keyword is reported before parsing, as for every source.
pub fn standalone_semantic_diagnostics(
    path: &Path,
    source: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Vec<SemanticDiagnostic>> {
    if let Some(diagnostic) = services::reserved_keyword_diagnostic(path, source) {
        return Ok(vec![diagnostic]);
    }
    let plan = services::synthetic_compile_plan_for_source(path);
    let resolved = services::resolved_input_from_plan(path.to_path_buf(), source.to_string(), plan, None, None);
    let (_, diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics(
        &resolved,
        services::PrepareOptions {
            front_end: services::FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..services::PrepareOptions::without_mods()
        },
        pipeline,
    )?;
    Ok(diagnostics)
}
