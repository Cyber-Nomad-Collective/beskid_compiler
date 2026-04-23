use crate::commands::analyze::AnalyzeArgs;
use crate::commands::build::BuildArgs;
use crate::commands::clif::ClifArgs;
use crate::commands::corelib::CorelibArgs;
use crate::commands::doc::DocArgs;
use crate::commands::fetch::FetchArgs;
use crate::commands::format::FormatArgs;
use crate::commands::lock::LockArgs;
use crate::commands::parse::ParseArgs;
use crate::commands::run::RunArgs;
use crate::commands::test::TestArgs;
use crate::commands::tree::TreeArgs;
use crate::commands::update::UpdateArgs;
use crate::commands::{
    analyze, build, clif, corelib, doc, fetch, format, lock, parse, run, test, tree, update,
};
use crate::corelib_runtime;
use beskid_pckg::PckgArgs;
use beskid_pckg::cli::PckgCommand;
use clap::{Parser, Subcommand};
use miette::Report;
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

    /// Emit API documentation (`api.json` + `index.md`) for a resolved source file
    Doc(DocArgs),

    /// Pretty-print Beskid sources using the canonical formatter
    #[command(visible_alias = "fmt")]
    Format(FormatArgs),

    /// Lower a Beskid file into CLIF and print the resulting IR
    Clif(ClifArgs),

    /// JIT-compile and execute a Beskid file
    Run(RunArgs),

    /// Discover and run Beskid `test` items
    Test(TestArgs),

    /// AOT-compile and link a Beskid file into object/library/executable outputs
    Build(BuildArgs),

    /// Resolve and materialize project dependencies
    Fetch(FetchArgs),

    /// Synchronize Project.lock for a project
    Lock(LockArgs),

    /// Update dependency resolution and materialized workspace
    Update(UpdateArgs),

    /// Materialize the checked-in Beskid corelib project template
    Corelib(CorelibArgs),

    /// Package-manager operations backed by the pckg service
    Pckg(PckgArgs),
}

pub fn run() -> miette::Result<()> {
    let os_args = env::args_os();
    let all_args =
        argfile::expand_args_from(os_args, argfile::parse_fromfile, argfile::PREFIX).unwrap();
    let cli = Cli::parse_from(all_args);
    ensure_corelib_ready().map_err(anyhow_to_miette)?;

    let result = match cli.command {
        Commands::Parse(args) => parse::execute(args),
        Commands::Tree(args) => tree::execute(args),
        Commands::Analyze(args) => analyze::execute(args),
        Commands::Doc(args) => doc::execute(args),
        Commands::Format(args) => format::execute(args),
        Commands::Clif(args) => clif::execute(args),
        Commands::Run(args) => run::execute(args),
        Commands::Test(args) => test::execute(args),
        Commands::Build(args) => build::execute(args),
        Commands::Fetch(args) => fetch::execute(args),
        Commands::Lock(args) => lock::execute(args),
        Commands::Update(args) => update::execute(args),
        Commands::Corelib(args) => corelib::execute(args),
        Commands::Pckg(args) => maybe_generate_docs_for_pack(&args)
            .and_then(|_| beskid_pckg::cli::execute(args).map_err(Into::into)),
    };

    result.map_err(anyhow_to_miette)
}

fn ensure_corelib_ready() -> anyhow::Result<()> {
    let provisioned = corelib_runtime::ensure_bundled_corelib()?;
    if provisioned.updated {
        println!(
            "corelib: updated to {} at {}",
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

fn maybe_generate_docs_for_pack(args: &PckgArgs) -> anyhow::Result<()> {
    let PckgCommand::Pack(pack_args) = &args.command else {
        return Ok(());
    };

    let source_root = absolutize_source_root(&pack_args.source)?;
    let (input, project) = resolve_doc_entrypoint(&source_root)?;
    let out = source_root.join(".beskid").join("docs");

    let doc_args = DocArgs {
        input,
        project,
        target: None,
        workspace_member: None,
        frozen: false,
        locked: false,
        out,
    };
    doc::execute(doc_args)?;
    Ok(())
}

fn absolutize_source_root(source: &Path) -> anyhow::Result<PathBuf> {
    if source.is_absolute() {
        return Ok(source.to_path_buf());
    }
    Ok(env::current_dir()?.join(source))
}

fn resolve_doc_entrypoint(source_root: &Path) -> anyhow::Result<(Option<PathBuf>, Option<PathBuf>)> {
    let project_manifest = source_root.join("Project.proj");
    if project_manifest.exists() {
        return Ok((None, Some(project_manifest)));
    }

    for candidate in [
        source_root.join("main.bd"),
        source_root.join("src").join("main.bd"),
        source_root.join("index.bd"),
    ] {
        if candidate.exists() {
            return Ok((Some(candidate), None));
        }
    }

    let mut bd_files: Vec<PathBuf> = WalkDir::new(source_root)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bd"))
        .collect();
    bd_files.sort();
    if bd_files.len() == 1 {
        return Ok((Some(bd_files.remove(0)), None));
    }

    anyhow::bail!(
        "cannot infer docs entrypoint for package source {} (expected Project.proj, main.bd/src/main.bd, or a single .bd file)",
        source_root.display()
    )
}
