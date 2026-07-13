//! `beskid import` — Foreign library import CLI (v0.3).
//!
//! Implements the `import lib` subcommand defined by the platform-spec
//! Foreign library import feature at
//! `site/website/src/content/docs/platform-spec/tooling/foreign-library-import/cli-import-lib-command.mdx`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use beskid_analysis::external_library::{
    ExternalLibraryRegistry, LibraryResolution, LibraryResolveError, current_host_key,
    default_registry, merge_resolution_into_manifest_source,
};
use beskid_analysis::projects::{
    discover_project_manifest_from_input_or_cwd, parse_manifest as parse_project_manifest,
};
use clap::{Args, Subcommand};

/// `beskid import` umbrella command.
#[derive(Args, Debug)]
pub struct ImportArgs {
    #[command(subcommand)]
    pub command: ImportCommand,
}

#[derive(Subcommand, Debug)]
pub enum ImportCommand {
    /// Import a foreign library into `Project.proj` link metadata
    Lib(LibArgs),
}

#[derive(Args, Debug)]
pub struct LibArgs {
    /// Logical library name (for example `libc`, `pthread`).
    pub logical: String,

    /// Choose ExternalLibrary provider (`c-posix` default on tier-1 hosts).
    #[arg(long, default_value = "c-posix")]
    pub provider: String,

    /// Resolve and print only; do not modify `Project.proj`.
    #[arg(long)]
    pub dry_run: bool,

    /// Path to `Project.proj` (default: discovered from cwd).
    #[arg(long)]
    pub project: Option<PathBuf>,
}

/// Dispatch the chosen `beskid import` subcommand.
pub fn execute(args: ImportArgs) -> Result<()> {
    match args.command {
        ImportCommand::Lib(lib_args) => execute_lib(lib_args),
    }
}

/// Resolves the logical library against the closed registry and merges the result into the link
/// block of `Project.proj`. Idempotent: re-running the same import is a no-op on disk.
pub fn execute_lib(args: LibArgs) -> Result<()> {
    let registry = default_registry();
    let resolution = resolve_with_registry(&registry, &args.provider, &args.logical)
        .map_err(library_resolve_error_to_anyhow)?;

    println!(
        "import: resolved `{}` via provider `{}` (host `{}`):",
        resolution.logical, resolution.provider, resolution.host_key,
    );
    for arg in &resolution.link_args {
        println!("  linker arg: {arg}");
    }
    for path in &resolution.search_paths {
        println!("  search path: {}", path.display());
    }

    if args.dry_run {
        println!("import: --dry-run set; Project.proj unchanged");
        return Ok(());
    }

    let manifest_path = resolve_manifest_path(args.project.as_deref())?;
    let source = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "failed to read project manifest at {}",
            manifest_path.display()
        )
    })?;

    let existing = parse_project_manifest(&source)
        .with_context(|| {
            format!(
                "failed to parse project manifest at {}",
                manifest_path.display()
            )
        })?
        .link;

    let outcome = merge_resolution_into_manifest_source(&source, existing.as_ref(), &resolution);

    if outcome.updated_source != source {
        fs::write(&manifest_path, &outcome.updated_source).with_context(|| {
            format!(
                "failed to write updated project manifest at {}",
                manifest_path.display()
            )
        })?;
        println!("import: updated link block in {}", manifest_path.display());
        if !outcome.added_libraries.is_empty() {
            println!(
                "import: added libraries: {}",
                outcome.added_libraries.join(", ")
            );
        }
        if !outcome.added_search_paths.is_empty() {
            let rendered = outcome
                .added_search_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!("import: added search paths: {rendered}");
        }
    } else {
        println!(
            "import: `{}` already present in {} (no-op)",
            args.logical,
            manifest_path.display()
        );
    }

    Ok(())
}

/// Resolve `logical` through `registry` using the provided `provider_id` and the runtime host
/// key. Surfaces structured `LibraryResolveError` values so the caller can print rich diagnostics.
pub fn resolve_with_registry(
    registry: &ExternalLibraryRegistry,
    provider_id: &str,
    logical: &str,
) -> Result<LibraryResolution, LibraryResolveError> {
    registry.resolve(provider_id, current_host_key(), logical)
}

fn library_resolve_error_to_anyhow(err: LibraryResolveError) -> anyhow::Error {
    anyhow!("{err}")
}

/// Resolve the project manifest path the same way `beskid lock` / `beskid fetch` do.
fn resolve_manifest_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        let candidate = expand_to_project_manifest(path)?;
        if !candidate.is_file() {
            bail!(
                "project manifest not found at {} (expected a `.bproj` manifest)",
                candidate.display()
            );
        }
        return Ok(candidate);
    }

    let discovered = discover_project_manifest_from_input_or_cwd(None, None)?;
    match discovered {
        Some((manifest_path, _)) => Ok(manifest_path),
        None => Err(anyhow!(
            "no `.bproj` manifest found from {}; pass --project or run inside a project directory",
            env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<cwd unavailable>".to_string())
        )),
    }
}

fn expand_to_project_manifest(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        return beskid_analysis::projects::discover_project_manifest_in_dir(path)
            .map_err(anyhow::Error::from)?
            .ok_or_else(|| anyhow!("no `.bproj` manifest found in {}", path.display()));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::external_library::known_provider_ids;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: crate::cli::Commands,
    }

    #[test]
    fn parses_import_lib_invocation() {
        let cli = TestCli::try_parse_from(["beskid", "import", "lib", "libc"]).expect("parse");
        match cli.cmd {
            crate::cli::Commands::Import(args) => match args.command {
                ImportCommand::Lib(lib_args) => {
                    assert_eq!(lib_args.logical, "libc");
                    assert_eq!(lib_args.provider, "c-posix");
                    assert!(!lib_args.dry_run);
                    assert!(lib_args.project.is_none());
                }
            },
            _ => panic!("expected Import command"),
        }
    }

    #[test]
    fn parses_import_lib_with_options() {
        let cli = TestCli::try_parse_from([
            "beskid",
            "import",
            "lib",
            "libc",
            "--provider",
            "posix",
            "--dry-run",
            "--project",
            "/tmp/example",
        ])
        .expect("parse");
        let crate::cli::Commands::Import(args) = cli.cmd else {
            panic!("expected Import command");
        };
        let ImportCommand::Lib(lib_args) = args.command;
        assert_eq!(lib_args.logical, "libc");
        assert_eq!(lib_args.provider, "posix");
        assert!(lib_args.dry_run);
        assert_eq!(lib_args.project, Some(PathBuf::from("/tmp/example")));
    }

    #[test]
    fn default_registry_advertises_closed_providers() {
        let ids = known_provider_ids();
        assert!(ids.contains(&"c-posix"));
        assert!(ids.contains(&"posix"));
        assert!(!ids.contains(&"msvc"));
    }
}
