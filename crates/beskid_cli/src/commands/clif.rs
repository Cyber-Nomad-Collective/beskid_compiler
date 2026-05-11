//! `beskid clif` — lower resolved Beskid source to CLIF and print the IR.

use crate::frontend;
use crate::pipeline_ui::resolve_input_with_cli_pipeline;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_codegen::{lower_source_with_pipeline, render_clif};
use beskid_pipeline::PipelineObserver;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ClifArgs {
    /// The input Beskid file to lower into CLIF
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Disable animated progress during resolve and lowering
    #[arg(long)]
    pub plain: bool,
}

/// Resolve and lower the program, then print rendered CLIF for the default entry.
pub fn execute(args: ClifArgs) -> Result<()> {
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

    let lowered = lower_source_with_pipeline(&resolved.source_path, &resolved.source, false, obs)?;
    print!("{}", render_clif(&lowered.artifact));

    Ok(())
}
