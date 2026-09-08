//! Root Clap model and subcommand dispatch for the `beskid` executable.

use crate::commands::analyze::AnalyzeArgs;
use crate::commands::build::BuildArgs;
use crate::commands::clif::ClifArgs;
use crate::commands::compiler_mod::ModArgs;
use crate::commands::corelib::CorelibArgs;
use crate::commands::doc::DocArgs;
use crate::commands::fetch::FetchArgs;
use crate::commands::format::FormatArgs;
use crate::commands::graph::GraphArgs;
use crate::commands::import::ImportArgs;
use crate::commands::lock::LockArgs;
use crate::commands::package::{PackageGetArgs, PackageInstallArgs, PackagePckgArgs, PackageRemoveArgs, PackageSearchArgs};
use crate::commands::publish::PublishArgs;
use crate::commands::lsp::LspArgs;
use crate::commands::migrate_bsol::MigrateBsolArgs;
use crate::commands::new::NewArgs;
use crate::commands::parse::ParseArgs;
use crate::commands::repl::ReplArgs;
use crate::commands::run::RunArgs;
use crate::commands::runtime_kit::RuntimeKitArgs;
use crate::commands::test::TestArgs;
use crate::commands::tree::TreeArgs;
use crate::commands::update::UpdateArgs;
use crate::commands::validate_bsol::ValidateBsolArgs;
use crate::commands::{
    analyze, build, clif, compiler_mod, corelib, doc, fetch, format, graph, import, lock, lsp, migrate_bsol, new,
    package, publish, parse, repl, run, runtime_kit, test, tree, update, validate_bsol,
};
use beskid_telemetry::{self, InitOptions};
use beskid_up::UpArgs;
use clap::{ArgAction, Parser, Subcommand};
use miette::Report;
use std::env;

/// Parsed `beskid` invocation (after `@file` argv expansion).
#[derive(Parser)]
#[command(name = "beskid")]
#[command(about = "Beskid CLI tool", version, author)]
pub struct Cli {
    /// Emit Cranelift JIT/codegen backend logs (also `BESKID_LOG_CRANELIFT=1`)
    #[arg(
        long = "log-cranelift",
        global = true,
        env = "BESKID_LOG_CRANELIFT",
        action = ArgAction::SetTrue,
        help = "Enable Cranelift JIT/codegen backend logs (default: off; see logging.rs)"
    )]
    pub log_cranelift: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// AOT-compile and execute a Beskid project.
    #[command(name = "run")]
    Run(RunArgs),

    /// Build release artifacts with target auto-detection.
    Publish(PublishArgs),

    /// Scaffold and instantiate projects from templates.
    #[command(name = "new")]
    New(Box<NewArgs>),

    /// Inspect package metadata and version history.
    Get(PackageGetArgs),

    /// Search the package registry.
    Search(PackageSearchArgs),

    /// Download package artifacts from the registry.
    Install(PackageInstallArgs),

    /// Remove package artifacts from the local install cache.
    #[command(name = "rm", alias = "remove")]
    Remove(PackageRemoveArgs),

    /// Advanced workflow surface; all non-stable commands remain here.
    Dev(DevCommand),
}

#[derive(Subcommand)]
pub enum DevCommand {
    /// Syntax-tree, formatting, and IR lowering workflows.
    Syntax(SyntaxDevCommand),

    /// Build and execution workflows.
    Build(BuildDevCommand),

    /// Project and dependency workflows.
    Project(ProjectDevCommand),

    /// Registry and package workflows.
    Package(PackageDevCommand),

    /// Runtime kit build workflows.
    #[command(name = "runtime-kit")]
    RuntimeKit(RuntimeKitArgs),

    /// Runtime, tooling, and maintenance workflows.
    Tooling(ToolingDevCommand),
}

#[derive(Subcommand)]
pub enum SyntaxDevCommand {
    Parse(ParseArgs),
    Tree(TreeArgs),
    Analyze(AnalyzeArgs),
    Doc(DocArgs),
    Format(FormatArgs),
    Clif(ClifArgs),
}

#[derive(Subcommand)]
pub enum BuildDevCommand {
    /// Compile outputs (executables, static libs, shared libs, or objects).
    #[command(name = "compile")]
    Compile(BuildArgs),
    Test(TestArgs),
    Mod(ModArgs),
    Corelib(CorelibArgs),
    Repl(ReplArgs),
}

#[derive(Subcommand)]
pub enum ProjectDevCommand {
    Fetch(FetchArgs),
    Lock(LockArgs),
    Update(UpdateArgs),
    Import(ImportArgs),
    Graph(GraphArgs),
}

#[derive(Subcommand)]
pub enum PackageDevCommand {
    /// Access full legacy package-server workflows.
    #[command(name = "pckg")]
    Pckg(PackagePckgArgs),
}

#[derive(Subcommand)]
pub enum ToolingDevCommand {
    Lsp(LspArgs),
    Up(UpArgs),
    #[command(name = "validate-bsol")]
    ValidateBsol(ValidateBsolArgs),
    #[command(name = "migrate-bsol")]
    MigrateBsol(MigrateBsolArgs),
}

/// Parses argv, provisions bundled corelib when needed, and runs the selected subcommand.
pub fn run() -> miette::Result<()> {
    let os_args = env::args_os();
    let all_args = argfile::expand_args_from(os_args, argfile::parse_fromfile, argfile::PREFIX).unwrap();
    let cli = Cli::parse_from(all_args);
    beskid_telemetry::init(InitOptions::cli(cli.log_cranelift));
    if !matches!(&cli.command, Commands::Dev(DevCommand::RuntimeKit(_))) {
        ensure_corelib_ready().map_err(anyhow_to_miette)?;
    }

    let result = match cli.command {
        Commands::Run(mut args) => {
            args.plain = true;
            run::execute(args)
        }
        Commands::Publish(args) => publish::execute(args),
        Commands::New(args) => new::execute(*args),
        Commands::Get(args) => package::get_command(args),
        Commands::Search(args) => package::search_command(args),
        Commands::Install(args) => package::install_command(args),
        Commands::Remove(args) => package::remove_command(args),
        Commands::Dev(args) => match args {
            DevCommand::Syntax(command) => match command {
                SyntaxDevCommand::Parse(args) => parse::execute(args),
                SyntaxDevCommand::Tree(args) => tree::execute(args),
                SyntaxDevCommand::Analyze(mut args) => {
                    args.plain = true;
                    analyze::execute(args)
                }
                SyntaxDevCommand::Doc(args) => doc::execute(args),
                SyntaxDevCommand::Format(args) => format::execute(args),
                SyntaxDevCommand::Clif(mut args) => {
                    args.plain = true;
                    clif::execute(args)
                }
            },
            DevCommand::Build(command) => match command {
                BuildDevCommand::Compile(mut args) => {
                    args.plain = true;
                    build::execute(args)
                }
                BuildDevCommand::Test(mut args) => {
                    args.plain = true;
                    test::execute(args)
                }
                BuildDevCommand::Mod(mut args) => {
                    match &mut args.command {
                        compiler_mod::ModCommand::Rebuild(args) => {
                            args.plain = true;
                        }
                        compiler_mod::ModCommand::Clean(args) => {
                            args.plain = true;
                        }
                    }
                    compiler_mod::execute(args)
                }
                BuildDevCommand::Corelib(args) => corelib::execute(args),
                BuildDevCommand::Repl(mut args) => {
                    args.plain = true;
                    repl::execute(args)
                }
            },
            DevCommand::RuntimeKit(args) => runtime_kit::execute(args),
            DevCommand::Project(command) => match command {
                ProjectDevCommand::Fetch(mut args) => {
                    args.progress.plain = true;
                    fetch::execute(args)
                }
                ProjectDevCommand::Lock(mut args) => {
                    args.progress.plain = true;
                    lock::execute(args)
                }
                ProjectDevCommand::Update(mut args) => {
                    args.progress.plain = true;
                    update::execute(args)
                }
                ProjectDevCommand::Import(args) => import::execute(args),
                ProjectDevCommand::Graph(args) => graph::execute(args),
            },
            DevCommand::Package(command) => match command {
                PackageDevCommand::Pckg(args) => {
                    package::execute_pckg_proxy(args)
                }
            },
            DevCommand::Tooling(command) => match command {
                ToolingDevCommand::Lsp(args) => lsp::execute(args),
                ToolingDevCommand::Up(args) => beskid_up::execute(args).map_err(anyhow::Error::from),
                ToolingDevCommand::ValidateBsol(args) => validate_bsol::execute(args),
                ToolingDevCommand::MigrateBsol(args) => migrate_bsol::execute(args),
            },
        },
    };

    result.map_err(anyhow_to_miette)
}

fn ensure_corelib_ready() -> anyhow::Result<()> {
    let provisioned = beskid_tools::ensure_bundled_corelib()?;
    if provisioned.updated {
        println!("corelib: updated to {} at {}", provisioned.version, provisioned.root.display());
    }
    Ok(())
}

fn anyhow_to_miette(error: anyhow::Error) -> Report {
    match error.downcast::<Report>() {
        Ok(report) => report,
        Err(error) => beskid_tools::diagnostics::report_from_anyhow(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_top_level_publish_command() {
        let cli = Cli::try_parse_from(["beskid", "publish", "main.bd"]).expect("parse cli");
        assert!(matches!(cli.command, Commands::Publish(_)));
    }

    #[test]
    fn parses_top_level_package_entrypoints() {
        let cli = Cli::try_parse_from(["beskid", "get", "corelib@0.1.2"]).expect("parse cli");
        assert!(matches!(cli.command, Commands::Get(_)));

        let cli = Cli::try_parse_from(["beskid", "search", "templates"]).expect("parse cli");
        assert!(matches!(cli.command, Commands::Search(_)));

        let cli = Cli::try_parse_from(["beskid", "install", "template.cli@1.0.0"]).expect("parse cli");
        assert!(matches!(cli.command, Commands::Install(_)));

        let cli = Cli::try_parse_from(["beskid", "rm", "template.cli@1.0.0"]).expect("parse cli");
        assert!(matches!(cli.command, Commands::Remove(_)));
    }

    #[test]
    fn parses_dev_syntax_tree() {
        let cli = Cli::try_parse_from(["beskid", "dev", "syntax", "tree", "main.bd"]).expect("parse cli");
        match cli.command {
            Commands::Dev(DevCommand::Syntax(SyntaxDevCommand::Tree(_))) => {}
            _ => panic!("expected `beskid dev syntax tree`"),
        }
    }

    #[test]
    fn parses_dev_project_graph() {
        let cli =
            Cli::try_parse_from(["beskid", "dev", "project", "graph", "main.bd"]).expect("parse cli");
        match cli.command {
            Commands::Dev(DevCommand::Project(ProjectDevCommand::Graph(_))) => {}
            _ => panic!("expected `beskid dev project graph`"),
        }
    }

    #[test]
    fn parses_dev_build_runtime_kit() {
        let cli = Cli::try_parse_from([
            "beskid",
            "dev",
            "runtime-kit",
            "build",
            "--prefix",
            "/tmp/out",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--profile",
            "debug",
            "--static-library",
            "out/libbeskid_runtime.a",
            "--shared-library",
            "out/libbeskid_runtime.so",
        ])
        .expect("parse cli");
        match cli.command {
            Commands::Dev(DevCommand::RuntimeKit(_)) => {}
            _ => panic!("expected `beskid dev runtime-kit build`"),
        }
    }

    #[test]
    fn parses_dev_build_compile() {
        let cli = Cli::try_parse_from(["beskid", "dev", "build", "compile", "main.bd"]).expect("parse cli");
        match cli.command {
            Commands::Dev(DevCommand::Build(BuildDevCommand::Compile(_))) => {}
            _ => panic!("expected `beskid dev build compile`"),
        }
    }

    #[test]
    fn parses_dev_package_pckg() {
        let cli = Cli::try_parse_from(["beskid", "dev", "package", "pckg", "whoami"]).expect("parse cli");
        match cli.command {
            Commands::Dev(DevCommand::Package(PackageDevCommand::Pckg(_))) => {}
            _ => panic!("expected `beskid dev package pckg`"),
        }
    }

    #[test]
    fn parses_no_top_level_pckg_command() {
        assert!(Cli::try_parse_from(["beskid", "pckg", "whoami"]).is_err());
    }
}
