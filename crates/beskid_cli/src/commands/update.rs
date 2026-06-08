//! `beskid update` — refresh dependency resolution and the materialized workspace tree.

use crate::project_args::{PlainProgressArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_tools::pipeline::{resolve_project_with_cli_pipeline, tui::CommandSummary};
use clap::Args;

#[derive(Args, Debug)]
pub struct UpdateArgs {
    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub progress: PlainProgressArgs,
}

/// Resolve with update semantics (non-frozen) and rematerialize dependencies.
pub fn execute(args: UpdateArgs) -> Result<()> {
    let (pipeline_ui, _resolved) = resolve_project_with_cli_pipeline(
        None,
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        false,
        false,
        args.progress.plain,
        UnresolvedDependencyPolicy::Warn,
    )?;
    pipeline_ui.finish_session_with_summary(
        "Workspace updated",
        Some(CommandSummary::plain("Update", "Workspace updated")),
    );
    println!("Dependency lock and materialized workspace updated.");
    Ok(())
}
