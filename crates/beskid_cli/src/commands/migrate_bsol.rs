//! Migrate BSOL documents between schema profile versions.

use std::path::PathBuf;

use anyhow::{Context, Result};
use bsol::migrate_document;
use clap::Parser;

/// Migrate a BSOL document to a target schema profile version.
#[derive(Debug, Parser)]
pub struct MigrateBsolArgs {
    /// Target schema profile (for example `project.v2`).
    #[arg(long)]
    pub to: String,

    /// Path to the BSOL document (defaults to stdin when omitted).
    pub path: Option<PathBuf>,

    /// Write migrated output to this path (defaults to stdout).
    #[arg(long, short)]
    pub output: Option<PathBuf>,
}

pub fn execute(args: MigrateBsolArgs) -> Result<()> {
    let source = match &args.path {
        Some(path) => std::fs::read_to_string(path)
            .with_context(|| format!("failed to read `{}`", path.display()))?,
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read stdin")?;
            buf
        }
    };

    let (migrated, _validated) = migrate_document(&source, &args.to)
        .map_err(|err| anyhow::Error::msg(err.to_string()))?;

    match &args.output {
        Some(path) => std::fs::write(path, &migrated)
            .with_context(|| format!("failed to write `{}`", path.display()))?,
        None => print!("{migrated}"),
    }

    Ok(())
}
