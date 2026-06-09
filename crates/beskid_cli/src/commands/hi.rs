//! `beskid hi` — pluggable dashboard shell.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use beskid_tools::shell::{ShellHost, ShellScope, WidgetRegistrar};

#[derive(Args, Debug)]
pub struct HiArgs {
    /// Optional path to resolve workspace/project scope (defaults to cwd)
    pub path: Option<PathBuf>,

    /// Disable terminal UI (print scope summary only)
    #[arg(long)]
    pub plain: bool,
}

pub fn execute(args: HiArgs, registrars: &[WidgetRegistrar]) -> Result<()> {
    let start = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let scope = ShellScope::resolve(&start);
    if args.plain {
        println!("beskid hi scope: {}", scope.label());
        return Ok(());
    }
    ShellHost::run_hi_blocking(scope, false, registrars)?;
    Ok(())
}
