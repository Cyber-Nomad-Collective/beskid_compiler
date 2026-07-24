//! Parse Beskid source to a spanned [`Program`](crate::syntax::Program), surfacing
//! [`MietteReportError`] on failure and optional parse-recovery diagnostics.

use anyhow::{Result, anyhow};
use pest::Parser;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::analysis::diagnostics::{MietteReportError, Severity, make_diagnostic};
use crate::parser::{BeskidParser, Rule};
use crate::parsing::parsable::Parsable;
use crate::syntax::{Program, Spanned};

use super::diagnostics_emit::{parse_error_diagnostic, pest_error_diagnostic};
use super::parse_recovery::collect_repair_candidates;

/// Result of parser recovery: always includes diagnostics captured while building the program.
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

/// Parse with diagnostics collected from strict and recovered parsing attempts.
pub fn parse_program_with_source_name_and_diagnostics(
    source_name: &str,
    source: &str,
) -> Result<ParsedProgram> {
    let strict_result = parse_program_strict(source_name, source);
    let strict_error = match strict_result {
        Ok(program) => {
            return Ok(ParsedProgram {
                program,
                diagnostics: Vec::new(),
                recovered: false,
            });
        }
        Err(err) => {
            if BeskidParser::parse(Rule::Program, source).is_ok() {
                return Err(err);
            }
            err
        }
    };

    let parse_error = match BeskidParser::parse(Rule::Program, source) {
        Ok(_) => return Err(strict_error),
        Err(err) => err,
    };
    let fallback = pest_error_diagnostic(source_name, source, &parse_error);

    for (candidate_source, mut parse_diagnostics) in
        collect_repair_candidates(source_name, source, &parse_error)
    {
        if let Ok(program) = parse_program_strict(source_name, &candidate_source) {
            if candidate_source == source {
                parse_diagnostics.clear();
            }
            return Ok(ParsedProgram {
                program,
                diagnostics: parse_diagnostics,
                recovered: candidate_source != source,
            });
        }
    }

    Err(anyhow!(MietteReportError::new(fallback)))
}

fn parse_program_strict(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|err| {
        let diagnostic = pest_error_diagnostic(source_name, source, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| {
        let end = if source.is_empty() {
            0
        } else {
            1.min(source.len())
        };
        let diagnostic = make_diagnostic(
            source_name,
            source,
            crate::syntax::SpanInfo {
                start: 0,
                end,
                line_col_start: (1, 1),
                line_col_end: (1, 1),
            },
            "no program found in source",
            "empty program",
            None,
            Some("parse".to_string()),
            Severity::Error,
        );
        anyhow::Error::new(MietteReportError::new(diagnostic))
    })?;
    Program::parse(pair).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })
}

/// Parse a single expression subtree (used by `code` literal `@{}` holes).
pub fn parse_expression_source(
    source_name: &str,
    source: &str,
) -> Result<Spanned<crate::syntax::Expression>> {
    let mut pairs = BeskidParser::parse(Rule::Expression, source.trim()).map_err(|err| {
        let diagnostic = pest_error_diagnostic(source_name, source, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| {
        anyhow!(MietteReportError::new(make_diagnostic(
            source_name,
            source,
            crate::syntax::SpanInfo {
                start: 0,
                end: 1.min(source.len()),
                line_col_start: (1, 1),
                line_col_end: (1, 1)
            },
            format!("no expression found in `{source_name}`"),
            "empty expression",
            None,
            Some("parse".to_string()),
            Severity::Error,
        )))
    })?;
    crate::syntax::expressions::expression::parse_expression(pair).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow!(MietteReportError::new(diagnostic))
    })
}
