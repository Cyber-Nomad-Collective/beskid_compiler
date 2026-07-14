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
use crate::commands::hi::HiArgs;
use crate::commands::import::ImportArgs;
use crate::commands::lock::LockArgs;
use crate::commands::lsp::LspArgs;
use crate::commands::migrate_bsol::MigrateBsolArgs;
use crate::commands::new::NewArgs;
use crate::commands::parse::ParseArgs;
use crate::commands::repl::ReplArgs;
use crate::commands::run::RunArgs;
use crate::commands::test::TestArgs;
use crate::commands::tree::TreeArgs;
use crate::commands::update::UpdateArgs;
use crate::commands::validate_bsol::ValidateBsolArgs;
use beskid_up::UpArgs;
use crate::commands::{
    analyze, build, clif, compiler_mod, corelib, doc, fetch, format, graph, hi, import, lock, lsp,
    migrate_bsol, new, parse, repl, run, test, tree, update, validate_bsol,
};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_pckg::PckgArgs;
use beskid_pckg::cli::PckgCommand;
use clap::{ArgAction, Parser, Subcommand};
use miette::Report;
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

    /// Import foreign libraries (currently `lib <name>`) into Project.proj `link` metadata
    Import(ImportArgs),

    /// Resolve and materialize project dependencies
    Fetch(FetchArgs),

    /// Synchronize Project.lock for a project
    Lock(LockArgs),

    /// Update dependency resolution and materialized workspace
    Update(UpdateArgs),

    /// Materialize the checked-in Beskid corelib project template
    Corelib(CorelibArgs),

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
    let all_args =
        argfile::expand_args_from(os_args, argfile::parse_fromfile, argfile::PREFIX).unwrap();
    let cli = Cli::parse_from(all_args);
    beskid_tools::logging::init(cli.log_cranelift);
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
        Commands::Repl(args) => repl::execute(args),
        Commands::Build(args) => build::execute(args),
        Commands::Mod(args) => compiler_mod::execute(args),
        Commands::Import(args) => import::execute(args),
        Commands::Fetch(args) => fetch::execute(args),
        Commands::Lock(args) => lock::execute(args),
        Commands::Update(args) => update::execute(args),
        Commands::Corelib(args) => corelib::execute(args),
        Commands::New(args) => new::execute(*args),
        Commands::Pckg(args) => maybe_generate_docs_for_pack(&args)
            .and_then(|_| beskid_pckg::cli::execute(args).map_err(Into::into)),
        Commands::Lsp(args) => lsp::execute(args),
        Commands::Up(args) => beskid_up::execute(args).map_err(anyhow::Error::from),
        Commands::ValidateBsol(args) => validate_bsol::execute(args),
        Commands::MigrateBsol(args) => migrate_bsol::execute(args),
        Commands::Graph(args) => graph::execute(args),
        Commands::Hi(args) => hi::execute(
            args,
            &[beskid_hi::register_widgets],
            &[beskid_hi::register_nav],
            &[],
        ),
    };

    result.map_err(anyhow_to_miette)
}

fn ensure_corelib_ready() -> anyhow::Result<()> {
    let provisioned = beskid_tools::ensure_bundled_corelib()?;
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
        Err(error) => beskid_tools::diagnostics::report_from_anyhow(&error),
    }
}

fn maybe_generate_docs_for_pack(args: &PckgArgs) -> anyhow::Result<()> {
    let PckgCommand::Pack(pack_args) = &args.command else {
        return Ok(());
    };

    let source_root = absolutize_source_root(&pack_args.source)?;
    if matches!(
        beskid_pckg::detect_pack_profile(&source_root)?,
        beskid_pckg::PackProfile::Template(_)
    ) {
        return Ok(());
    }
    let (input, project) = resolve_doc_entrypoint(&source_root)?;
    let out = source_root.join(".beskid").join("docs");

    let doc_args = DocArgs {
        input,
        project: ProjectResolveArgs {
            project,
            target: None,
            workspace_member: None,
        },
        lockfile: LockfilePolicyArgs {
            frozen: false,
            locked: false,
        },
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

fn resolve_doc_entrypoint(
    source_root: &Path,
) -> anyhow::Result<(Option<PathBuf>, Option<PathBuf>)> {
    if let Ok(Some(project_manifest)) =
        beskid_analysis::projects::discover_project_manifest_in_dir(source_root)
    {
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
        "cannot infer docs entrypoint for package source {} (expected a `.bproj` manifest, main.bd/src/main.bd, or a single .bd file)",
        source_root.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_up_list() {
        let parsed = Cli::try_parse_from(["beskid", "up", "list"]).unwrap();
        assert!(matches!(parsed.command, Commands::Up(_)));
    }

    #[test]
    fn parses_mod_rebuild_with_clean_project_and_target() {
        let cli = Cli::try_parse_from([
            "beskid",
            "mod",
            "rebuild",
            "--clean",
            "--target-triple",
            "aarch64-apple-darwin",
            "mods/MyMod",
        ])
        .expect("parse cli");

        let Commands::Mod(args) = cli.command else {
            panic!("expected mod command");
        };
        let crate::commands::compiler_mod::ModCommand::Rebuild(args) = args.command else {
            panic!("expected rebuild command");
        };
        assert!(args.clean);
        assert_eq!(args.target_triple.as_deref(), Some("aarch64-apple-darwin"));
        assert_eq!(args.project.as_deref(), Some(Path::new("mods/MyMod")));
    }

    #[test]
    fn parses_new_list_with_online() {
        let cli = Cli::try_parse_from(["beskid", "new", "list", "--online"]).expect("parse");
        let Commands::New(args) = cli.command else {
            panic!("expected new command");
        };
        let Some(crate::commands::new::NewCommand::List(list)) = args.command else {
            panic!("expected list subcommand");
        };
        assert!(list.online);
    }

    #[test]
    fn parses_new_console_instantiate() {
        let cli = Cli::try_parse_from([
            "beskid",
            "new",
            "console",
            "-n",
            "MyApp",
            "-o",
            "./MyApp",
            "--no-interactive",
        ])
        .expect("parse");
        let Commands::New(args) = cli.command else {
            panic!("expected new");
        };
        assert_eq!(args.short_name.as_deref(), Some("console"));
        assert_eq!(args.instantiate.name.as_deref(), Some("MyApp"));
    }

    #[test]
    fn parses_lsp_install_with_release_tag() {
        let cli = Cli::try_parse_from(["beskid", "lsp", "install", "--release-tag", "lsp-v0.1.5"])
            .expect("parse cli");
        let Commands::Lsp(args) = cli.command else {
            panic!("expected lsp command");
        };
        let Some(crate::commands::lsp::LspCommand::Install(install)) = args.command else {
            panic!("expected lsp install");
        };
        assert_eq!(install.release_tag, "lsp-v0.1.5");
    }

    #[test]
    fn parses_mod_clean_with_project() {
        let cli = Cli::try_parse_from(["beskid", "mod", "clean", "mods/MyMod"]).expect("parse cli");

        let Commands::Mod(args) = cli.command else {
            panic!("expected mod command");
        };
        let crate::commands::compiler_mod::ModCommand::Clean(args) = args.command else {
            panic!("expected clean command");
        };
        assert_eq!(args.project.as_deref(), Some(Path::new("mods/MyMod")));
    }
}
