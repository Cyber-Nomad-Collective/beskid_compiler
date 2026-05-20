//! `beskid run` — JIT-compile the resolved program and invoke an entrypoint.

use std::path::PathBuf;

use crate::frontend;
use crate::pipeline_ui::{
    PipelineProgressKind, resolve_input_with_cli_pipeline_kind,
};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_engine::services::run_entrypoint_with_pipeline;
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
    frontend::run_semantic_analysis_gate(
        &resolved.source_path,
        &resolved.source,
        None,
        pipeline_ui.as_ref(),
    )?;
    pipeline_ui.finish_prepare_ui("Analysis complete");

    let output = run_entrypoint_with_pipeline(
        &resolved.source_path,
        &resolved.source,
        &args.entrypoint,
        None,
    )?;
    pipeline_ui.finish_session("Run complete");
    println!("{output}");

    Ok(())
}
