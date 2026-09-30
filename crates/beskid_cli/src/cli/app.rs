use crate::commands::analyze::AnalyzeArgs;
use crate::commands::build::BuildArgs;
use crate::commands::clif::ClifArgs;
use crate::commands::compiler_mod::ModArgs;
use crate::commands::corelib::CorelibArgs;
use crate::commands::doc::DocArgs;
use crate::commands::fetch::FetchArgs;
use crate::commands::format::FormatArgs;
use crate::commands::graph::GraphArgs;
use crate::commands::hi::HiArgs;
use crate::commands::import::ImportArgs;
use crate::commands::lock::LockArgs;
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
    analyze, build, clif, compiler_mod, corelib, doc, fetch, format, graph, hi, import, lock, lsp, migrate_bsol, new,
    parse, repl, run, runtime_kit, test, tree, update, validate_bsol,
};
use beskid_pckg::PckgArgs;
use beskid_telemetry::{self, InitOptions};
use beskid_up::UpArgs;
use clap::{ArgAction, Parser, Subcommand};
use std::env;

use super::dev::{DevArgs, DevBuildCommand, DevCommand, DevPackageCommand, DevProjectCommand, DevSyntaxCommand};
use super::docs::{anyhow_to_miette, ensure_corelib_ready, maybe_generate_docs_for_pack};

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
    /// Developer-oriented command groups; concise root commands remain available as shortcuts
    Dev(DevArgs),

    /// Parse a Beskid file and output the AST representation
    Parse(ParseArgs),

    /// Generate an AST visualization tree from a Beskid file
    Tree(TreeArgs),

    /// Run semantic analysis (builtin rules) and print diagnostics for a Beskid source file
    Analyze(AnalyzeArgs),

    /// Emit API documentation (`api.json` + `index.md`) for a resolved source file
    Doc(DocArgs),

    /// Pretty-print Beskid sources using the canonical formatter
    #[command(visible_alias = "fmt")]
    Format(FormatArgs),

    /// Lower a Beskid file into CLIF and print the resulting IR
    Clif(ClifArgs),

    /// AOT-compile and execute a Beskid file in a subprocess
    Run(RunArgs),

    /// Discover and run Beskid `test` items
    Test(TestArgs),

    /// Evaluate expression/statement snippets in an interactive JIT REPL
    Repl(ReplArgs),

    /// AOT-compile and link a Beskid file into object/library/executable outputs
    Build(BuildArgs),

    /// Manage compiler Mod AOT artifacts
    Mod(ModArgs),

    /// Import foreign libraries (currently `lib <name>`) into the project `.bproj` `link` metadata
    Import(ImportArgs),

    /// Resolve and materialize project dependencies
    Fetch(FetchArgs),

    /// Synchronize Project.lock for a project
    Lock(LockArgs),

    /// Update dependency resolution and materialized workspace
    Update(UpdateArgs),

    /// Materialize the checked-in Beskid corelib project template
    Corelib(CorelibArgs),

    /// Build and install target/profile-specific ABI-v5 native runtime kits
    RuntimeKit(RuntimeKitArgs),

    /// Scaffold projects from templates (`list`, `install`, `uninstall`, or `<shortName>`)
    New(Box<NewArgs>),

    /// Package-manager operations backed by the pckg service
    Pckg(PckgArgs),

    /// Visualize project/workspace graphs (Mermaid) in the terminal or as raw output
    Graph(GraphArgs),

    /// Open the pluggable Beskid dashboard shell (workspace/project/user scoped)
    Hi(HiArgs),

    /// Run the Beskid language server on stdio, or install a release binary (`beskid lsp install`)
    Lsp(LspArgs),

    /// Check and manage direct-download Beskid toolchain versions
    Up(UpArgs),

    /// Validate a BSOL document against a schema profile
    #[command(name = "validate-bsol")]
    ValidateBsol(ValidateBsolArgs),

    /// Migrate a BSOL document to a newer schema profile version
    #[command(name = "migrate-bsol")]
    MigrateBsol(MigrateBsolArgs),
}

/// Parses argv, provisions bundled corelib when needed, and runs the selected subcommand.
pub fn run() -> miette::Result<()> {
    let os_args = env::args_os();
    let all_args = argfile::expand_args_from(os_args, argfile::parse_fromfile, argfile::PREFIX).unwrap();
    let cli = Cli::parse_from(all_args);
    beskid_telemetry::init(InitOptions::cli(cli.log_cranelift));
    if !matches!(&cli.command, Commands::RuntimeKit(_)) {
        ensure_corelib_ready().map_err(anyhow_to_miette)?;
    }

    let result = match cli.command {
        Commands::Dev(args) => execute_dev(args),
        Commands::Parse(args) => parse::execute(args),
        Commands::Tree(args) => tree::execute(args),
        Commands::Analyze(args) => analyze::execute(args),
        Commands::Doc(args) => doc::execute(args),
        Commands::Format(args) => format::execute(args),
        Commands::Clif(args) => clif::execute(args),
        Commands::Run(args) => run::execute(args),
        Commands::Test(args) => test::execute(args),
        Commands::Repl(args) => repl::execute(args),
        Commands::Build(args) => build::execute(args),
        Commands::Mod(args) => compiler_mod::execute(args),
        Commands::Import(args) => import::execute(args),
        Commands::Fetch(args) => fetch::execute(args),
        Commands::Lock(args) => lock::execute(args),
        Commands::Update(args) => update::execute(args),
        Commands::Corelib(args) => corelib::execute(args),
        Commands::RuntimeKit(args) => runtime_kit::execute(args),
        Commands::New(args) => new::execute(*args),
        Commands::Pckg(args) => execute_pckg(args),
        Commands::Lsp(args) => lsp::execute(args),
        Commands::Up(args) => beskid_up::execute(args).map_err(anyhow::Error::from),
        Commands::ValidateBsol(args) => validate_bsol::execute(args),
        Commands::MigrateBsol(args) => migrate_bsol::execute(args),
        Commands::Graph(args) => graph::execute(args),
        Commands::Hi(args) => hi::execute(args, &[beskid_hi::register_widgets], &[beskid_hi::register_nav], &[]),
    };

    result.map_err(anyhow_to_miette)
}

fn execute_dev(args: DevArgs) -> anyhow::Result<()> {
    match args.command {
        DevCommand::Syntax(args) => match args.command {
            DevSyntaxCommand::Parse(args) => parse::execute(args),
            DevSyntaxCommand::Tree(args) => tree::execute(args),
            DevSyntaxCommand::Analyze(args) => analyze::execute(args),
            DevSyntaxCommand::Doc(args) => doc::execute(args),
            DevSyntaxCommand::Format(args) => format::execute(args),
            DevSyntaxCommand::Clif(args) => clif::execute(args),
        },
        DevCommand::Build(args) => match args.command {
            DevBuildCommand::Compile(args) => build::execute(args),
            DevBuildCommand::Test(args) => test::execute(args),
            DevBuildCommand::Corelib(args) => corelib::execute(args),
        },
        DevCommand::Project(args) => match args.command {
            DevProjectCommand::Fetch(args) => fetch::execute(args),
            DevProjectCommand::Lock(args) => lock::execute(args),
            DevProjectCommand::Update(args) => update::execute(args),
            DevProjectCommand::Graph(args) => graph::execute(args),
        },
        DevCommand::Package(args) => match args.command {
            DevPackageCommand::Registry(args) => execute_pckg(args),
        },
    }
}

pub(super) fn execute_pckg(args: PckgArgs) -> anyhow::Result<()> {
    maybe_generate_docs_for_pack(&args).and_then(|_| beskid_pckg::cli::execute(args).map_err(Into::into))
}
