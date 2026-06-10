//! Parse Beskid source to a spanned [`Program`](crate::syntax::Program), surfacing [`MietteReportError`] on failure.

use anyhow::Result;
use pest::Parser;

use crate::analysis::diagnostics::MietteReportError;
use crate::parser::{BeskidParser, Rule};
use crate::parsing::parsable::Parsable;
use crate::syntax::{Program, Spanned};

use super::diagnostics_emit::{parse_error_diagnostic, pest_error_diagnostic};

/// Parse in-memory source named `"<memory>"` for stack traces and diagnostics.
pub fn parse_program(source: &str) -> Result<Spanned<Program>> {
    parse_program_with_source_name("<memory>", source)
}

/// Parse with a stable `source_name` (file path or synthetic label) for diagnostics.
pub fn parse_program_with_source_name(source_name: &str, source: &str) -> Result<Spanned<Program>> {
    let mut pairs = BeskidParser::parse(Rule::Program, source).map_err(|err| {
        let diagnostic = pest_error_diagnostic(source_name, source, &err);
        anyhow::Error::new(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| {
        let end = if source.is_empty() {
            0
        } else {
            1.min(source.len())
        };
        let diagnostic = crate::analysis::diagnostics::make_diagnostic(
            source_name,
            source,
            crate::syntax::SpanInfo {
                start: 0,
                end,
                line_col_start: (1, 1),
                line_col_end: (1, 1),
            },
            "no program found in source",
            "parse",
            None,
            Some("parse".to_string()),
            crate::analysis::Severity::Error,
        );
        anyhow::Error::new(MietteReportError::new(diagnostic))
    })?;
    Program::parse(pair).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow::Error::new(MietteReportError::new(diagnostic))
    })
}

/// Parse a single expression subtree (used by `code` literal `@{}` holes).
pub fn parse_expression_source(
    source_name: &str,
    source: &str,
) -> Result<Spanned<crate::syntax::Expression>> {
    let mut pairs = BeskidParser::parse(Rule::Expression, source.trim()).map_err(|err| {
        let diagnostic = pest_error_diagnostic(source_name, source, &err);
        anyhow::Error::new(MietteReportError::new(diagnostic))
    })?;
    let pair = pairs.next().ok_or_else(|| {
        anyhow::anyhow!("no expression found in `{source_name}`")
    })?;
    crate::syntax::expressions::expression::parse_expression(pair).map_err(|err| {
        let diagnostic = parse_error_diagnostic(source_name, source, &err);
        anyhow::Error::new(MietteReportError::new(diagnostic))
    })
}
