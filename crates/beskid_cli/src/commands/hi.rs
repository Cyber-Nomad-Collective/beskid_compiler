//! `beskid hi` — pluggable dashboard shell.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use beskid_tools::shell::{
    NavRegistrar, ShellHost, ShellScope, ToolSettingsRegistrar, WidgetRegistrar,
};

#[derive(Args, Debug)]
pub struct HiArgs {
    /// Optional path to resolve workspace/project scope (defaults to cwd)
    pub path: Option<PathBuf>,

    /// Disable terminal UI (print scope summary only)
    #[arg(long)]
    pub plain: bool,
}

pub fn execute(
    args: HiArgs,
    widget_registrars: &[WidgetRegistrar],
    nav_registrars: &[NavRegistrar],
    settings_registrars: &[ToolSettingsRegistrar],
) -> Result<()> {
    let start = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let scope = ShellScope::resolve_cwd(&start);
    if args.plain {
        println!("beskid hi scope: {}", scope.label());
        return Ok(());
    }
    ShellHost::run_hi_blocking(
        scope,
        false,
        widget_registrars,
        nav_registrars,
        settings_registrars,
        Some(super::hi_compile::run_hi_compile),
    )?;
    Ok(())
}
