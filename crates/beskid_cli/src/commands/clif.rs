//! `beskid clif` — lower resolved Beskid source to CLIF and print the IR.

use crate::frontend;
use crate::pipeline_ui::{PipelineProgressKind, resolve_input_with_cli_pipeline_kind};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_codegen::{lower_source_with_pipeline, render_clif};
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

    let lowered = lower_source_with_pipeline(&resolved.source_path, &resolved.source, false, None)?;
    pipeline_ui.finish_session("CLIF ready");
    print!("{}", render_clif(&lowered.artifact));

    Ok(())
}
