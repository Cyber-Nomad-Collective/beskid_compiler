//! `beskid lock` — synchronize `Project.lock` for the selected project/workspace.

use crate::pipeline_ui::resolve_project_with_cli_pipeline;
use crate::project_args::{PlainProgressArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use clap::Args;

#[derive(Args, Debug)]
pub struct LockArgs {
    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub progress: PlainProgressArgs,
}

/// Resolve the project and refresh the lockfile (non-frozen, non-locked policy).
pub fn execute(args: LockArgs) -> Result<()> {
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
    pipeline_ui.finish_session("Project.lock synchronized");
    println!("Project.lock synchronized.");
    Ok(())
}
