//! Shared Clap argument groups for project / workspace / lockfile resolution.

use clap::Args;
use std::path::PathBuf;

// Shared project, workspace and target selection.
#[derive(Args, Debug, Clone)]
pub struct ProjectResolveArgs {
    /// Path to a project directory or `.bproj` manifest file
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Target name from the project manifest
    #[arg(long)]
    pub target: Option<String>,

    /// Workspace member name when resolving from a `.bws` workspace
    #[arg(long = "workspace-member")]
    pub workspace_member: Option<String>,
}

// Shared animated-progress preference; nonempty NO_COLOR also disables animation.
#[derive(Args, Debug, Clone, Copy, Default)]
pub struct PlainProgressArgs {
    /// Disable animated progress output
    #[arg(long)]
    pub plain: bool,
}

// Shared lock and network policy for resolution commands.
#[derive(Args, Debug, Clone)]
pub struct LockfilePolicyArgs {
    /// Combine locked resolution with offline verified-cache-only resolution
    #[arg(long)]
    pub frozen: bool,

    /// Require lockfile to exist and match resolution
    #[arg(long)]
    pub locked: bool,

    /// Forbid network requests and require verified cached dependency artifacts
    #[arg(long)]
    pub offline: bool,
}

impl LockfilePolicyArgs {
    #[allow(non_snake_case)]
    pub fn WorkspaceOptions(&self) -> beskid_analysis::projects::WorkspacePrepareOptions {
        beskid_analysis::projects::WorkspacePrepareOptions {
            frozen: self.frozen,
            locked: self.locked,
            offline: self.offline,
            refresh_lock: false,
        }
    }
}
