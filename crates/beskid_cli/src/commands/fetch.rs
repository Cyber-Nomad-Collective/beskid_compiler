//! `beskid fetch` — resolve the project graph and materialize dependencies on disk.

use crate::pipeline_ui::resolve_project_with_cli_pipeline;
use crate::project_args::{LockfilePolicyArgs, PlainProgressArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use clap::Args;

#[derive(Args, Debug)]
pub struct FetchArgs {
    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    #[command(flatten)]
    pub progress: PlainProgressArgs,
}

/// Resolve with unresolved-deps warnings and materialize the workspace (see `Project.lock`).
pub fn execute(args: FetchArgs) -> Result<()> {
    let (pipeline_ui, resolved) = resolve_project_with_cli_pipeline(
        None,
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.progress.plain,
        UnresolvedDependencyPolicy::Warn,
    )?;
    pipeline_ui.finish_session("Dependencies resolved and materialized");
    println!("Dependencies resolved and materialized.");
    Ok(())
}
