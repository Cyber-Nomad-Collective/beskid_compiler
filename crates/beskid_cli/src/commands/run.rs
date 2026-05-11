//! `beskid run` — JIT-compile the resolved program and invoke an entrypoint.

use std::path::PathBuf;

use crate::frontend;
use crate::pipeline_ui::resolve_input_with_cli_pipeline;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_engine::services::run_entrypoint_with_pipeline;
use beskid_pipeline::PipelineObserver;
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
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.plain,
    )?;
    let obs: Option<&dyn PipelineObserver> = Some(pipeline_ui.as_ref());
    frontend::validate_source(&resolved.source_path, &resolved.source)?;

    let output = run_entrypoint_with_pipeline(
        &resolved.source_path,
        &resolved.source,
        &args.entrypoint,
        obs,
    )?;
    println!("{output}");

    Ok(())
}
