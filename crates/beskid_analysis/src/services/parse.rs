//! Parse Beskid source to a spanned [`Program`](crate::syntax::Program), surfacing
//! [`MietteReportError`] on failure; a failed parse carries one structured parse diagnostic.

use anyhow::{Result, anyhow};
use pest::Parser;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::analysis::diagnostics::{MietteReportError, Severity, make_diagnostic};
use crate::parser::{BeskidParser, Rule};
use crate::parsing::diagnose::{EngineRepair, codes, diagnose_parse_failure, refine_with_engine_repair};
use crate::parsing::parsable::Parsable;
use crate::syntax::{Program, Spanned};

use super::diagnostics_emit::parse_error_diagnostic;
use super::parse_recovery::recover_with_repair_candidates;

/// A parsed program and the parse errors found on the way.
///
/// When the source does not parse, `diagnostics` holds the error (with a fix where the repair has
/// one meaning) and `program` is the tree of the repaired source; `recovered` is then `true`.
#[derive(Debug, Clone)]
pub struct ParsedProgram {
    pub program: Spanned<Program>,
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub recovered: bool,
}

/// Parse in-memory source named `"<memory>"` for stack traces and diagnostics.
pub fn parse_program(source: &str) -> Result<Spanned<Program>> {
    parse_program_with_source_name_and_diagnostics("<memory>", source).map(|parsed| parsed.program)
}

/// Parse with a stable `source_name` (file path or synthetic label) for diagnostics.
pub fn parse_program_with_source_name(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    parse_program_with_source_name_and_diagnostics(source_name, source).map(|parsed| parsed.program)
}

/// Parse, and on a grammar error report it and try to continue with a repaired source.
pub fn parse_program_with_source_name_and_diagnostics(source_name: &str, source: &str) -> Result<ParsedProgram> {
    let parse_error = match BeskidParser::parse(Rule::Program, source) {
        Ok(_) => {
            let program = parse_program_strict(source_name, source)?;
            return Ok(ParsedProgram { program, diagnostics: Vec::new(), recovered: false });
        }
        Err(err) => err,
    };

    let diagnosis = diagnose_parse_failure(source_name, source, Rule::Program, &parse_error);
    if let Some(repaired) = &diagnosis.repaired_source
        && let Ok(program) = parse_program_strict(source_name, repaired)
    {
        return Ok(ParsedProgram { program, diagnostics: vec![diagnosis.diagnostic], recovered: true });
    }

    if let Some((program, repair)) = recover_with_repair_candidates(source, &parse_error, |candidate_source| {
        parse_program_strict(source_name, candidate_source)
    }) {
        let repair = match &repair {
            Some(candidate) => candidate.as_engine_repair(),
            None => EngineRepair::Other,
        };
        let diagnosis = refine_with_engine_repair(diagnosis, source, Rule::Program, repair);
        return Ok(ParsedProgram { program, diagnostics: vec![diagnosis.diagnostic], recovered: true });
    }

    Err(anyhow!(MietteReportError::new(diagnosis.diagnostic)))
}

fn parse_program_strict(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|err| {
        let diagnostic = diagnose_parse_failure(source_name, source, Rule::Program, &err).diagnostic;
        anyhow!(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| anyhow!(MietteReportError::new(empty_source(source_name, source))))?;
    crate::syntax::common::span::with_line_index(source, || Program::parse(pair)).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })
}

fn empty_source(source_name: &str, source: &str) -> SemanticDiagnostic {
    let end = if source.is_empty() { 0 } else { 1.min(source.len()) };
    make_diagnostic(
        source_name,
        source,
        crate::syntax::SpanInfo { start: 0, end, line_col_start: (1, 1), line_col_end: (1, 1) },
        "nothing to parse",
        "empty source",
        None,
        Some(codes::EMPTY_SOURCE.to_string()),
        Severity::Error,
    )
}

/// Parse a single expression subtree (used by `code` literal `@{}` holes).
pub fn parse_expression_source(source_name: &str, source: &str) -> Result<Spanned<crate::syntax::Expression>> {
    let trimmed = source.trim();
    let mut pairs = BeskidParser::parse(Rule::Expression, trimmed).map_err(|err| {
        let diagnostic = diagnose_parse_failure(source_name, trimmed, Rule::Expression, &err).diagnostic;
        anyhow!(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| anyhow!(MietteReportError::new(empty_source(source_name, source))))?;
    crate::syntax::common::span::with_line_index(trimmed, || {
        crate::syntax::expressions::expression::parse_expression(pair)
    })
    .map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, trimmed, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })
}
