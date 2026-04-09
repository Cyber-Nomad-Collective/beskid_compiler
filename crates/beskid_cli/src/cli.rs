use crate::commands::analyze::AnalyzeArgs;
use crate::commands::build::BuildArgs;
use crate::commands::clif::ClifArgs;
use crate::commands::fetch::FetchArgs;
use crate::commands::lock::LockArgs;
use crate::commands::parse::ParseArgs;
use crate::commands::run::RunArgs;
use crate::commands::stdlib::StdlibArgs;
use crate::commands::tree::TreeArgs;
use crate::commands::update::UpdateArgs;
use crate::commands::{analyze, build, clif, fetch, lock, parse, run, stdlib, tree, update};
use crate::stdlib_runtime;
use beskid_pckg::PckgArgs;
use clap::{Parser, Subcommand};
use miette::Report;
use std::env;

#[derive(Parser)]
#[command(name = "beskid")]
#[command(about = "Beskid CLI tool", version, author)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Parse a Beskid file and output the AST representation
    Parse(ParseArgs),

    /// Generate an AST visualization tree from a Beskid file
    Tree(TreeArgs),

    /// Run semantic analysis (builtin rules) and print diagnostics for a Beskid source file
    Analyze(AnalyzeArgs),

    /// Lower a Beskid file into CLIF and print the resulting IR
    Clif(ClifArgs),

    /// JIT-compile and execute a Beskid file
    Run(RunArgs),

    /// AOT-compile and link a Beskid file into object/library/executable outputs
    Build(BuildArgs),

    /// Resolve and materialize project dependencies
    Fetch(FetchArgs),

    /// Synchronize Project.lock for a project
    Lock(LockArgs),

    /// Update dependency resolution and materialized workspace
    Update(UpdateArgs),

    /// Materialize the checked-in Beskid standard library project template
    Stdlib(StdlibArgs),

    /// Package-manager operations backed by the pckg service
    Pckg(PckgArgs),
}

pub fn run() -> miette::Result<()> {
    let os_args = env::args_os();
    let all_args =
        argfile::expand_args_from(os_args, argfile::parse_fromfile, argfile::PREFIX).unwrap();
    let cli = Cli::parse_from(all_args);
    ensure_stdlib_ready().map_err(anyhow_to_miette)?;

    let result = match cli.command {
        Commands::Parse(args) => parse::execute(args),
        Commands::Tree(args) => tree::execute(args),
        Commands::Analyze(args) => analyze::execute(args),
        Commands::Clif(args) => clif::execute(args),
        Commands::Run(args) => run::execute(args),
        Commands::Build(args) => build::execute(args),
        Commands::Fetch(args) => fetch::execute(args),
        Commands::Lock(args) => lock::execute(args),
        Commands::Update(args) => update::execute(args),
        Commands::Stdlib(args) => stdlib::execute(args),
        Commands::Pckg(args) => beskid_pckg::cli::execute(args).map_err(Into::into),
    };

    result.map_err(anyhow_to_miette)
}

fn ensure_stdlib_ready() -> anyhow::Result<()> {
    let provisioned = stdlib_runtime::ensure_bundled_stdlib()?;
    if provisioned.updated {
        println!(
            "stdlib: updated to {} at {}",
            provisioned.version,
            provisioned.root.display()
        );
    }
    Ok(())
}

fn anyhow_to_miette(error: anyhow::Error) -> Report {
    match error.downcast::<Report>() {
        Ok(report) => report,
        Err(error) => miette::miette!("{error:#}"),
    }
}
