//! In-process `build` / `test` for `beskid hi` (pipeline progress stays in the shell).

use anyhow::Result;
use clap::{Parser, Subcommand};

use beskid_tools::shell::{HiCompileRequest, ShellScope};

use super::analyze::AnalyzeArgs;
use super::build::BuildArgs;
use super::graph::GraphArgs;
use super::run::RunArgs;
use super::test::TestArgs;

#[derive(Parser)]
#[command(name = "beskid")]
struct CliWrap {
    #[command(subcommand)]
    command: HiSubcommand,
}

#[derive(Subcommand)]
enum HiSubcommand {
    Analyze(AnalyzeArgs),
    Build(BuildArgs),
    Graph(GraphArgs),
    Run(RunArgs),
    Test(TestArgs),
}

pub fn run_hi_compile(req: HiCompileRequest<'_>) -> Result<()> {
    match req.command {
        "build" => {
            let args = parse_build_args(req.params, req.scope)?;
            super::build::execute_for_hi(req.msg_tx, args)
        }
        "run" => {
            let args = parse_run_args(req.params, req.scope)?;
            super::run::execute_for_hi(req.msg_tx, args)
        }
        "test" => {
            let args = parse_test_args(req.params, req.scope)?;
            super::test::execute_for_hi(req.msg_tx, args)
        }
        "analyze" => {
            let args = parse_analyze_args(req.params, req.scope)?;
            super::analyze::execute_for_hi(req.msg_tx, args)
        }
        "graph" => {
            let args = parse_graph_args(req.params, req.scope)?;
            super::graph::execute_for_hi(req.msg_tx, args)
        }
        other => anyhow::bail!("unsupported hi compile command: {other}"),
    }
}

fn parse_build_args(params: &str, scope: &ShellScope) -> Result<BuildArgs> {
    let argv = argv_for_subcommand("build", params, scope);
    match CliWrap::try_parse_from(argv)?.command {
        HiSubcommand::Build(args) => Ok(args),
        _ => anyhow::bail!("expected build subcommand"),
    }
}

fn parse_test_args(params: &str, scope: &ShellScope) -> Result<TestArgs> {
    let argv = argv_for_subcommand("test", params, scope);
    match CliWrap::try_parse_from(argv)?.command {
        HiSubcommand::Test(args) => Ok(args),
        _ => anyhow::bail!("expected test subcommand"),
    }
}

fn parse_run_args(params: &str, scope: &ShellScope) -> Result<RunArgs> {
    let argv = argv_for_subcommand("run", params, scope);
    match CliWrap::try_parse_from(argv)?.command {
        HiSubcommand::Run(args) => Ok(args),
        _ => anyhow::bail!("expected run subcommand"),
    }
}

fn parse_analyze_args(params: &str, scope: &ShellScope) -> Result<super::analyze::AnalyzeArgs> {
    let argv = argv_for_subcommand("analyze", params, scope);
    match CliWrap::try_parse_from(argv)?.command {
        HiSubcommand::Analyze(args) => Ok(args),
        _ => anyhow::bail!("expected analyze subcommand"),
    }
}

fn parse_graph_args(params: &str, scope: &ShellScope) -> Result<super::graph::GraphArgs> {
    let argv = argv_for_subcommand("graph", params, scope);
    match CliWrap::try_parse_from(argv)?.command {
        HiSubcommand::Graph(args) => Ok(args),
        _ => anyhow::bail!("expected graph subcommand"),
    }
}

fn argv_for_subcommand(subcmd: &str, params: &str, scope: &ShellScope) -> Vec<String> {
    let mut argv = vec!["beskid".to_string(), subcmd.to_string()];
    if params.trim().is_empty() {
        scope.append_project_argv(&mut argv);
    } else {
        argv.extend(params.split_whitespace().map(str::to_string));
    }
    argv
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc;

    use beskid_tools::pipeline::PipelineProgressKind;
    use beskid_tools::session::{CommandSession, ResolveInputArgs};

    use super::*;

    #[test]
    fn argv_for_build_with_scope_uses_project_flag() {
        let scope = ShellScope::Project {
            root: PathBuf::from("/tmp/myproj"),
            manifest: PathBuf::from("/tmp/myproj/app.bproj"),
        };
        let argv = argv_for_subcommand("build", "", &scope);
        assert!(argv.windows(2).any(|w| w == ["--project", "/tmp/myproj/app.bproj"]));
        assert!(!argv.iter().any(|a| a == "/tmp/myproj"));
    }

    #[test]
    fn hi_compile_corelib_mvp_resolve_uses_entry_file() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let manifest = manifest_dir.join("../beskid_e2e_tests/fixtures/corelib_mvp/CorelibMvp.bproj");
        if !manifest.is_file() {
            eprintln!("skip hi_compile_corelib_mvp_resolve_uses_entry_file: {manifest:?} missing");
            return;
        }
        let root = manifest.parent().expect("fixture root").to_path_buf();
        let scope = ShellScope::Project { root: root.clone(), manifest: manifest.clone() };
        let args = parse_build_args("", &scope).expect("parse build args");
        let resolve_args = ResolveInputArgs {
            input: args.input.as_ref(),
            project: args.project.project.as_ref(),
            target: args.project.target.as_deref(),
            workspace_member: args.project.workspace_member.as_deref(),
            frozen: args.lockfile.frozen,
            locked: args.lockfile.locked,
        };
        let (tx, _rx) = mpsc::channel();
        let session = CommandSession::with_attached_pipeline(tx, PipelineProgressKind::FullBuild);
        let resolved = session.resolve_input(&resolve_args).expect("resolve");
        assert!(resolved.source_path.is_file(), "expected entry file, got {}", resolved.source_path.display());
        assert!(
            !resolved.source_path.to_string_lossy().contains("Failed to read file"),
            "resolve should not treat workspace root as source"
        );
    }
}
