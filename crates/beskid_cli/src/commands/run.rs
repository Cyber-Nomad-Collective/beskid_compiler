//! `beskid run` — JIT-compile the resolved program and invoke an entrypoint.

use std::path::PathBuf;

use crate::pipeline_ui::{PipelineProgressKind, resolve_input_with_cli_pipeline_kind};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The input Beskid file to JIT-compile and execute
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Entrypoint function name
    #[arg(long, default_value = "main")]
    pub entrypoint: String,

    /// Disable animated progress and graph output
    #[arg(long)]
    pub plain: bool,
}

/// Resolve, JIT, and run `args.entrypoint` with pipeline progress on stderr when enabled.
pub fn execute(args: RunArgs) -> Result<()> {
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline_kind(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.plain,
        PipelineProgressKind::PrepareAndRun,
    )?;
    pipeline_ui.show_build_graph(&resolved);
    pipeline_ui.halt_progress_bars_for_output();

    let (_, gate_diagnostics) = beskid_analysis::services::prepare_compilation_diagnostics(
        &resolved,
        beskid_analysis::services::PrepareOptions {
            mode: beskid_analysis::services::PrepareMode::DiagnosticsOnly,
            front_end: beskid_analysis::services::FrontEndOptions {
                with_semantic_diagnostics: true,
                ..Default::default()
            },
        },
        Some(pipeline_ui.as_ref()),
    )?;
    pipeline_ui.report_semantic_diagnostics(&gate_diagnostics);
    beskid_analysis::services::require_no_semantic_errors(&gate_diagnostics)
        .map_err(anyhow::Error::from)?;
    pipeline_ui.finish_prepare_ui("Analysis complete");

    let output = beskid_engine::services::run_resolved_entrypoint_after_gate_with_pipeline(
        &resolved,
        &args.entrypoint,
        Some(pipeline_ui.as_ref()),
    )?;
    pipeline_ui.finish_session("Run complete");
    println!("{output}");

    Ok(())
}
