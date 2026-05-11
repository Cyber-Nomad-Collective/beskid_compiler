//! Shared Clap argument groups for project / workspace / lockfile resolution.

use clap::Args;
use std::path::PathBuf;

/// Flags shared by subcommands that resolve `Project.proj` / workspace / target selection.
#[derive(Args, Debug, Clone)]
pub struct ProjectResolveArgs {
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

/// Disable animated progress; also implied when `NO_COLOR` is set to a non-empty value.
/// Opt out of animated progress (some commands flatten this into their own `Args`).
#[derive(Args, Debug, Clone, Copy, Default)]
pub struct PlainProgressArgs {
    /// Disable animated progress output
    #[arg(long)]
    pub plain: bool,
}

/// `--frozen` / `--locked` lockfile behavior for resolution commands.
#[derive(Args, Debug, Clone)]
pub struct LockfilePolicyArgs {
    /// Require lockfile to be up to date and forbid lockfile updates
    #[arg(long)]
    pub frozen: bool,

    /// Require lockfile to exist and match resolution
    #[arg(long)]
    pub locked: bool,
}
