#![allow(non_snake_case)]
use crate::commands::{
    analyze, build, clif, compiler_mod, corelib, doc, fetch, format, graph, import, lock, lsp, migrate_bsol, new,
    parse, repl, run, runtime_kit, test, tree, validate_bsol,
};
use crate::commands::{
    analyze::AnalyzeArgs, build::BuildArgs, doc::DocArgs, format::FormatArgs, new::NewArgs, run::RunArgs,
    test::TestArgs,
};
use beskid_pckg::PckgArgs;
use beskid_telemetry::{self, InitOptions};
use clap::{Parser, Subcommand};
use std::env;

use super::dev::{DevArgs, DevCommand, DevProjectCommand, DevSyntaxCommand};
use super::docs::{anyhow_to_miette, ensure_corelib_ready, maybe_generate_docs_for_pack};

/// Parsed canonical user-task invocation.
#[derive(Parser)]
#[command(name = "beskid", about = "beskid application tooling", version, author)]
#[command(
    after_help = "First project:\n  beskid new hello\n  cd hello\n  beskid add <package>[@<version>]\n  beskid run\n\nUse `beskid dev --help` for compiler inspection and maintenance."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
#[derive(Subcommand)]
pub enum Commands {
    /// Create a project with the bundled offline application template.
    New(Box<NewArgs>),
    /// Check semantics and manifest validity without linking.
    Check(AnalyzeArgs),
    /// Compile a project or source file.
    Build(BuildArgs),
    /// Build and run an application.
    Run(RunArgs),
    /// Build and run selected tests.
    Test(TestArgs),
    /// Format Beskid source.
    Fmt(FormatArgs),
    /// Generate API documentation.
    Doc(DocArgs),
    /// Add a project dependency and resolve its lock transactionally.
    Add(crate::commands::dependency::AddArgs),
    /// Remove a project dependency.
    Remove(crate::commands::dependency::RemoveArgs),
    /// Update selected project dependencies.
    Update(crate::commands::dependency::UpdateArgs),
    /// Find, manage and publish packages or templates.
    Package(crate::commands::package::PackageArgs),
    /// Inspect or update the installed toolchain.
    Toolchain(crate::commands::toolchain::ToolchainArgs),
    /// Diagnose installation without changing it.
    Doctor(crate::commands::doctor::DoctorArgs),
    /// Compiler inspection and maintenance.
    Dev(DevArgs),
}

/// Parse with actionable migration errors and no legacy dispatch.
pub fn ParseInvocation<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<std::ffi::OsString>>();
    match Cli::try_parse_from(&args) {
        Ok(cli) => Ok(cli),
        Err(error) if error.kind() == clap::error::ErrorKind::InvalidSubcommand => {
            let old = args.get(1).and_then(|arg| arg.to_str());
            let replacement = match old {
                Some("analyze") => Some("check"),
                Some("format") => Some("fmt"),
                Some("pckg") => Some("package"),
                Some("up") => Some("toolchain"),
                Some("parse") => Some("dev syntax parse"),
                Some("tree") => Some("dev syntax tree"),
                Some("clif") => Some("dev syntax clif"),
                Some("fetch") => Some("dev project fetch"),
                Some("lock") => Some("dev project lock"),
                Some("graph") => Some("dev project graph"),
                Some("mod") => Some("dev mod"),
                Some("import") => Some("dev import"),
                Some("lsp") => Some("dev lsp"),
                Some("runtime-kit") => Some("dev runtime-kit"),
                Some("corelib") => Some("dev corelib"),
                Some("repl") => Some("dev repl"),
                Some("validate-bsol") => Some("dev bsol validate"),
                Some("migrate-bsol") => Some("dev bsol migrate"),
                _ => None,
            };
            if let Some(replacement) = replacement {
                Err(clap::Error::raw(
                    clap::error::ErrorKind::InvalidSubcommand,
                    format!(
                        "`beskid {}` was removed; use `beskid {replacement}`. Run `beskid {replacement} --help` for current arguments.\n",
                        old.unwrap()
                    ),
                ))
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

pub fn run() -> miette::Result<()> {
    let args = argfile::expand_args_from(env::args_os(), argfile::parse_fromfile, argfile::PREFIX)
        .map_err(|error| miette::miette!("cannot expand argument file: {error}"))?;
    let cli = ParseInvocation(args).unwrap_or_else(|error| error.exit());
    let backend_logs = matches!(&cli.command, Commands::Dev(args) if args.log_cranelift);
    beskid_telemetry::init(InitOptions::cli(backend_logs));
    if NeedsCorelib(&cli.command) {
        ensure_corelib_ready().map_err(anyhow_to_miette)?;
    }
    let result = match cli.command {
        Commands::New(args) => new::execute(*args),
        Commands::Check(args) => analyze::execute(args),
        Commands::Build(args) => build::execute(args),
        Commands::Run(args) => run::execute(args),
        Commands::Test(args) => test::execute(args),
        Commands::Fmt(args) => format::execute(args),
        Commands::Doc(args) => doc::execute(args),
        Commands::Add(args) => crate::commands::dependency::Add(args),
        Commands::Remove(args) => crate::commands::dependency::Remove(args),
        Commands::Update(args) => crate::commands::dependency::Update(args),
        Commands::Package(args) => crate::commands::package::execute(args),
        Commands::Toolchain(args) => crate::commands::toolchain::execute(args),
        Commands::Doctor(args) => crate::commands::doctor::execute(args),
        Commands::Dev(args) => execute_dev(args),
    };
    result.map_err(anyhow_to_miette)
}

pub(super) fn NeedsCorelib(command: &Commands) -> bool {
    match command {
        Commands::Check(_) | Commands::Build(_) | Commands::Run(_) | Commands::Test(_) | Commands::Doc(_) => true,
        Commands::Dev(args) => matches!(
            args.command,
            DevCommand::Build(_)
                | DevCommand::Mod(_)
                | DevCommand::Repl(_)
                | DevCommand::Syntax(super::dev::DevSyntaxArgs { command: DevSyntaxCommand::Clif(_) })
                | DevCommand::Project(_)
        ),
        Commands::Package(args) => {
            matches!(&args.command, crate::commands::package::PackageCommand::Pack(pack) if !pack.skip_docs)
        }
        _ => false,
    }
}
fn execute_dev(args: DevArgs) -> anyhow::Result<()> {
    match args.command {
        DevCommand::Syntax(args) => match args.command {
            DevSyntaxCommand::Parse(args) => parse::execute(args),
            DevSyntaxCommand::Tree(args) => tree::execute(args),
            DevSyntaxCommand::Clif(args) => clif::execute(args),
        },
        DevCommand::Build(args) => build::execute(args),
        DevCommand::Project(args) => match args.command {
            DevProjectCommand::Fetch(args) => fetch::execute(args),
            DevProjectCommand::Lock(args) => lock::execute(args),
            DevProjectCommand::Graph(args) => graph::execute(args),
        },
        DevCommand::Corelib(args) => corelib::execute(args),
        DevCommand::RuntimeKit(args) => runtime_kit::execute(args),
        DevCommand::Mod(args) => compiler_mod::execute(args),
        DevCommand::Import(args) => import::execute(args),
        DevCommand::Lsp(args) => lsp::execute(args),
        DevCommand::Repl(args) => repl::execute(args),
        DevCommand::Bsol(args) => match args.command {
            super::dev::DevBsolCommand::Validate(args) => validate_bsol::execute(args),
            super::dev::DevBsolCommand::Migrate(args) => migrate_bsol::execute(args),
        },
        DevCommand::NativeModWorker(args) => beskid_analysis::mod_host::run_native_mod_worker(&args.endpoint),
        DevCommand::ToolchainOwner(args) => crate::commands::toolchain_owner::execute(args),
        DevCommand::Capabilities(_) => {
            use clap::CommandFactory;
            let root = Cli::command();
            let commands = root
                .get_subcommands()
                .filter(|command| command.get_name() != "help")
                .map(|command| command.get_name())
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::json!({"schemaVersion":1,"cliVersion":env!("CARGO_PKG_VERSION"),"commands":commands,"outputSchemas":[]})
            );
            Ok(())
        }
    }
}
pub(crate) fn execute_pckg(args: PckgArgs) -> anyhow::Result<()> {
    maybe_generate_docs_for_pack(&args).and_then(|_| beskid_pckg::cli::execute(args).map_err(Into::into))
}
