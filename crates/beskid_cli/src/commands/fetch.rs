//! `beskid fetch` — resolve the project graph and materialize dependencies on disk.

use crate::project_args::{LockfilePolicyArgs, PlainProgressArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_tools::pipeline::{
    CliProjectPipelineOptions, CliResolveOptions, resolve_project_with_cli_pipeline, tui::CommandSummary,
};
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
    let (pipeline_ui, _resolved) = resolve_project_with_cli_pipeline(CliProjectPipelineOptions {
        resolve: CliResolveOptions::new(
            None,
            args.project.project.as_ref(),
            args.project.target.as_deref(),
            args.project.workspace_member.as_deref(),
            args.lockfile.frozen,
            args.lockfile.locked,
            args.progress.plain,
        ),
        unresolved_dependency_policy: UnresolvedDependencyPolicy::Warn,
    })?;
    pipeline_ui.finish_session_with_summary(
        "Dependencies resolved and materialized",
        Some(CommandSummary::plain("Fetch", "Dependencies resolved and materialized")),
    );
    println!("Dependencies resolved and materialized.");
    Ok(())
}
