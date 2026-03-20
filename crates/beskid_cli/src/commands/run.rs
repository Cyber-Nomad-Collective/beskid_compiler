use anyhow::Result;
use beskid_analysis::services;
use beskid_analysis::parser::{BeskidParser, Rule};
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::syntax::Program;
use crate::errors as cli_errors;
use pest::Parser;
use pest::iterators::Pairs;
use beskid_engine::services::run_entrypoint;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The input Beskid file to JIT-compile and execute
    pub input: Option<PathBuf>,

    /// Path to a project directory or Project.proj file
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Target name from Project.proj
    #[arg(long)]
    pub target: Option<String>,

    /// Workspace member name when resolving from Workspace.proj
    #[arg(long = "workspace-member")]
    pub workspace_member: Option<String>,

    /// Require lockfile to be up to date and forbid lockfile updates
    #[arg(long)]
    pub frozen: bool,

    /// Require lockfile to exist and match resolution
    #[arg(long)]
    pub locked: bool,

    /// Entrypoint function name
    #[arg(long, default_value = "main")]
    pub entrypoint: String,
}

pub fn execute(args: RunArgs) -> Result<()> {
    let resolved = services::resolve_input(
        args.input.as_ref(),
        args.project.as_ref(),
        args.target.as_deref(),
        args.workspace_member.as_deref(),
        args.frozen,
        args.locked,
    )?;
    // Pre-parse and pretty-print any pest/parse errors via miette before lowering/executing
    match BeskidParser::parse(Rule::Program, &resolved.source) {
        Ok(mut pairs) => {
            if let Some(pair) = pairs.next() {
                if let Err(err) = Program::parse(pair) {
                    cli_errors::print_pretty_parse_error(
                        &resolved.source_path.display().to_string(),
                        &resolved.source,
                        &err,
                    );
                    return Ok(());
                }
            }
        }
        Err(err) => {
            cli_errors::print_pretty_pest_error(
                &resolved.source_path.display().to_string(),
                &resolved.source,
                &err,
            );
            return Ok(());
        }
    }

    let output = run_entrypoint(&resolved.source_path, &resolved.source, &args.entrypoint)?;
    println!("{output}");

    Ok(())
}
