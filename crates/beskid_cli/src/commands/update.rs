use anyhow::Result;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_analysis::services;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Path to a project directory or Project.proj file
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Target name from Project.proj
    #[arg(long)]
    pub target: Option<String>,

    /// Workspace member name when resolving from Workspace.proj
    #[arg(long = "workspace-member")]
    pub workspace_member: Option<String>,
}

pub fn execute(args: UpdateArgs) -> Result<()> {
    let _ = services::resolve_project_with_policy(
        None,
        args.project.as_ref(),
        args.target.as_deref(),
        args.workspace_member.as_deref(),
        false,
        false,
        UnresolvedDependencyPolicy::Warn,
    )?;
    println!("Dependency lock and materialized workspace updated.");
    Ok(())
}
