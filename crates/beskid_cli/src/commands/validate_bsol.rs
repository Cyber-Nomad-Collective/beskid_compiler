//! Validate BSOL documents against schema profiles.

use std::path::PathBuf;

use anyhow::{Context, Result};
use bsol::{AnalysisOptions, AnalysisSession};
#[cfg(test)]
use bsol::{load_profile, parse_bsol_document, validate};
use bsol_beskid_bridge::PckgSchemaSource;
use clap::Parser;

/// Validate a BSOL document against a schema profile.
#[derive(Debug, Parser)]
pub struct ValidateBsolArgs {
    /// Schema profile name (for example `project.v1`, `workspace.v1`, `board.v2`).
    #[arg(long, default_value = "project.v1")]
    pub profile: String,

    /// Apply profile migration rewrites before validation.
    #[arg(long)]
    pub migrate: bool,

    /// Path to the BSOL document (defaults to stdin when omitted).
    pub path: Option<PathBuf>,
}

pub fn execute(args: ValidateBsolArgs) -> Result<()> {
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

    let base_dir = args
        .path
        .as_ref()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let mut session = AnalysisSession::new();
    if let Ok(cache) = std::env::var("BESKID_SCHEMA_CACHE") {
        session.add_schema_source(Box::new(PckgSchemaSource::new(cache)));
    }
    let options = AnalysisOptions::for_profile(&args.profile)
        .with_base_dir(base_dir)
        .with_migrate(args.migrate);
    session
        .analyze_source(&source, &options)
        .map_err(|err| anyhow::Error::msg(err.to_string()))?;

    eprintln!("ok: validated against profile `{}`", args.profile);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_minimal_manifest_fixture() {
        let src = r#"demo {
  name = "demo"
  version = "0.1.0"
  root = "."
}
"#;
        parse_bsol_document(src).expect("parse");
        let profile = load_profile("project.v1").expect("profile");
        validate(&parse_bsol_document(src).unwrap(), &profile).expect("validate");
    }
}
