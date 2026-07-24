//! `beskid lsp` — run or install the Beskid language server.

use anyhow::Result;
use beskid_tools::toolchain::release::{InstallLspOptions, install_lsp, managed_lsp_exists, managed_lsp_path};
use clap::{Args, Subcommand};
use std::process::Command;

#[derive(Args, Debug)]
pub struct LspArgs {
    #[command(subcommand)]
    pub command: Option<LspCommand>,
}

#[derive(Subcommand, Debug)]
pub enum LspCommand {
    /// Download `beskid_lsp` from GitHub releases into `~/.beskid/bin`.
    Install(LspInstallArgs),
}

#[derive(Args, Debug)]
pub struct LspInstallArgs {
    /// GitHub release tag (default rolling `lsp-latest`; pin with `lsp-vX.Y.Z`).
    #[arg(long, default_value = "lsp-latest")]
    pub release_tag: String,
}

/// Start the LSP server on stdin/stdout, or run an install subcommand.
pub fn execute(args: LspArgs) -> Result<()> {
    match args.command {
        Some(LspCommand::Install(install)) => {
            install_lsp(&InstallLspOptions { release_tag: install.release_tag })?;
            Ok(())
        }
        None => run_stdio_server(),
    }
}

fn run_stdio_server() -> Result<()> {
    if managed_lsp_exists() {
        let path = managed_lsp_path()?;
        let status = Command::new(&path).status().with_context_spawn(&path)?;
        if !status.success() {
            anyhow::bail!("managed language server exited with {}", status.code().unwrap_or(-1));
        }
        return Ok(());
    }

    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(beskid_lsp::run_stdio_server())
}

trait WithContextSpawn {
    fn with_context_spawn(self, path: &std::path::Path) -> Result<std::process::ExitStatus>;
}

impl WithContextSpawn for std::io::Result<std::process::ExitStatus> {
    fn with_context_spawn(self, path: &std::path::Path) -> Result<std::process::ExitStatus> {
        self.map_err(|error| anyhow::anyhow!("failed to run {}: {error}", path.display()))
    }
}
