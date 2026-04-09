use std::path::{Path, PathBuf};

use anyhow::Result;
use beskid_analysis::parser::{BeskidParser, Rule};
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::services;
use beskid_analysis::syntax::{Program, Spanned};
use pest::Parser;

pub fn resolve_input(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
) -> Result<services::ResolvedInput> {
    services::resolve_input(input, project, target, workspace_member, frozen, locked)
}

pub fn parse_program(path: &Path, source: &str) -> Result<Spanned<Program>> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|err| {
        let diagnostic = services::pest_error_diagnostic(&path.display().to_string(), source, &err);
        anyhow::anyhow!("{:?}", miette::Report::new(diagnostic))
    })?;
    let pair = pairs
        .next()
        .ok_or_else(|| anyhow::anyhow!("No program found"))?;
    Program::parse(pair).map_err(|err| {
        let diagnostic =
            services::parse_error_diagnostic(&path.display().to_string(), source, &err);
        anyhow::anyhow!("{:?}", miette::Report::new(diagnostic))
    })
}

pub fn validate_source(path: &Path, source: &str) -> Result<()> {
    let _ = parse_program(path, source)?;
    Ok(())
}
