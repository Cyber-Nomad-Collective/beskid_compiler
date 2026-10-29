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

impl ProjectResolveArgs {
    /// Sets `target` to the target `command` selects when no `--target` was given. Fails when the
    /// manifest has no candidate or more than one candidate of the selected kind.
    pub fn select_default_target(
        &mut self,
        input: Option<&PathBuf>,
        command: beskid_analysis::projects::DefaultTargetCommand,
    ) -> anyhow::Result<()> {
        if self.target.is_none() {
            self.target = beskid_analysis::services::default_target_for_input(
                input,
                self.project.as_ref(),
                self.workspace_member.as_deref(),
                command,
            )?;
        }
        Ok(())
    }
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
    pub fn workspace_options(&self) -> beskid_analysis::projects::WorkspacePrepareOptions {
        beskid_analysis::projects::WorkspacePrepareOptions {
            frozen: self.frozen,
            locked: self.locked,
            offline: self.offline,
            refresh_lock: false,
        }
    }
}
