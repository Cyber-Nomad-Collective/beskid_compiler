//! In-process `build` / `test` for `beskid hi` (pipeline progress stays in the shell).

use anyhow::Result;
use clap::{Parser, Subcommand};

use beskid_tools::shell::{HiCompileRequest, ShellScope};

use super::build::BuildArgs;
use super::test::TestArgs;

#[derive(Parser)]
#[command(name = "beskid")]
struct CliWrap {
    #[command(subcommand)]
    command: HiSubcommand,
}

#[derive(Subcommand)]
enum HiSubcommand {
    Build(BuildArgs),
    Test(TestArgs),
}

pub fn run_hi_compile(req: HiCompileRequest<'_>) -> Result<()> {
    match req.command {
        "build" => {
            let args = parse_build_args(req.params, req.scope)?;
            super::build::execute_for_hi(req.msg_tx, args)
        }
        "test" => {
            let args = parse_test_args(req.params, req.scope)?;
            super::test::execute_for_hi(req.msg_tx, args)
        }
        other => anyhow::bail!("unsupported hi compile command: {other}"),
    }
}

fn parse_build_args(params: &str, scope: &ShellScope) -> Result<BuildArgs> {
  let argv = argv_for_subcommand("build", params, scope);
  match CliWrap::try_parse_from(argv)?.command {
    HiSubcommand::Build(args) => Ok(args),
    HiSubcommand::Test(_) => anyhow::bail!("expected build subcommand"),
  }
}

fn parse_test_args(params: &str, scope: &ShellScope) -> Result<TestArgs> {
  let argv = argv_for_subcommand("test", params, scope);
  match CliWrap::try_parse_from(argv)?.command {
    HiSubcommand::Test(args) => Ok(args),
    HiSubcommand::Build(_) => anyhow::bail!("expected test subcommand"),
  }
}

fn argv_for_subcommand(subcmd: &str, params: &str, scope: &ShellScope) -> Vec<String> {
    let mut argv = vec!["beskid".to_string(), subcmd.to_string()];
    if params.trim().is_empty() {
        if let Some(root) = scope.root_dir() {
            argv.push(root.display().to_string());
        }
    } else {
        argv.extend(params.split_whitespace().map(str::to_string));
    }
    argv
}
