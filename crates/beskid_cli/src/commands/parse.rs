//! `beskid parse` — parse a `.bd` file and dump the AST (debug format).

use anyhow::{Context, Result};
use beskid_analysis::services;
use clap::Args;
use std::fs;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ParseArgs {
    /// The input Beskid file to parse
    #[arg(required = true)]
    pub input: PathBuf,

    /// Output format: debug (json not yet supported)
    #[arg(long, value_parser = ["debug"], default_value = "debug")]
    pub format: String,
}

/// Read `args.input`, parse, and print `Debug` AST output.
pub fn execute(args: ParseArgs) -> Result<()> {
    let source =
        fs::read_to_string(&args.input).with_context(|| format!("Failed to read file: {}", args.input.display()))?;
    let program = services::parse_program_with_source_name(&args.input.display().to_string(), &source)?;

    let _ = args.format.as_str();
    println!("{:#?}", program.node);
    Ok(())
}
