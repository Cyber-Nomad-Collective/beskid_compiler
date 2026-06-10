//! CLI subprocess planning and execution (TUI callers suspend the terminal first).

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::catalog::{CliCommandDef, CommandItem};
use super::scope::ShellScope;

/// Arguments for a single subprocess launch after the TUI releases the terminal.
#[derive(Debug, Clone)]
pub struct CliRunPlan {
    pub exe: PathBuf,
    pub args: Vec<String>,
}

pub fn plan_cli_command(
    exe: &PathBuf,
    item: &CommandItem,
    params: &str,
    scope: &ShellScope,
) -> Option<CliRunPlan> {
    let CommandItem::Cli(cli) = item else {
        return None;
    };
    Some(CliRunPlan {
        exe: exe.clone(),
        args: build_argv(cli, params, scope),
    })
}

/// Plan an external shell command (first argv token is the executable).
pub fn plan_external_command(argv: Vec<String>, params: &str) -> Option<CliRunPlan> {
    if argv.is_empty() {
        return None;
    }
    let mut parts = argv;
    if !params.trim().is_empty() {
        parts.extend(params.split_whitespace().map(str::to_string));
    }
    let exe = PathBuf::from(&parts[0]);
    let args = parts[1..].to_vec();
    Some(CliRunPlan { exe, args })
}

fn build_argv(cli: &CliCommandDef, params: &str, scope: &ShellScope) -> Vec<String> {
    let mut args: Vec<String> = cli.argv_prefix.iter().map(|s| (*s).to_string()).collect();
    if !params.trim().is_empty() {
        args.extend(params.split_whitespace().map(str::to_string));
    } else if let Some(root) = scope.root_dir() {
        args.push(root.display().to_string());
    }
    args
}

pub fn run_cli_plan(plan: &CliRunPlan) -> io::Result<()> {
    let status = Command::new(&plan.exe)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        let label = plan
            .args
            .first()
            .map(String::as_str)
            .unwrap_or("beskid");
        writeln!(
            io::stderr(),
            "command `{label}` exited with {}",
            status.code().unwrap_or(-1)
        )?;
    }
    Ok(())
}
