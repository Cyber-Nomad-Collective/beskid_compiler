//! `beskid clif` — lower resolved Beskid source to CLIF and print the IR.

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_codegen::{lower_resolved_entrypoint_with_pipeline, render_clif};
use beskid_tools::PipelineProgressKind;
use beskid_tools::session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
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
    let resolve_args = ResolveInputArgs {
        input: args.input.as_ref(),
        project: args.project.project.as_ref(),
        target: args.project.target.as_deref(),
        workspace_member: args.project.workspace_member.as_deref(),
        frozen: args.lockfile.frozen,
        locked: args.lockfile.locked,
    };
    let (session, resolved) = CommandSession::open_and_resolve(
        args.plain,
        PipelineProgressKind::PrepareAndRun,
        &resolve_args,
    )?;
    session.semantic_gate(&resolved, SemanticGateOptions::default())?;

    let lowered = lower_resolved_entrypoint_with_pipeline(
        &resolved,
        Some("main"),
        false,
        Some(session.observer()),
    )?;
    session.pipeline().finish_session("CLIF ready");
    print!("{}", render_clif(&lowered.artifact));

    Ok(())
}
