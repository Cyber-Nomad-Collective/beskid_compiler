use pest::error::InputLocation;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::projects::ProjectError;

fn escape_bsol_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
}

pub fn bsol_error(code: &str, message: &str) -> String {
    format!(
        "Error {code} {{\n  Message = \"{}\";\n}}",
        escape_bsol_text(message)
    )
}

pub fn pest_error_diagnostic(
    source_name: &str,
    source: &str,
    err: &pest::error::Error<Rule>,
) -> SemanticDiagnostic {
    let start = match err.location {
        InputLocation::Pos(pos) => pos,
        InputLocation::Span((start, _)) => start,
    };
    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        crate::syntax::SpanInfo {
            start,
            end: start.saturating_add(1),
            line_col_start: (1, 1),
            line_col_end: (1, 1),
        },
        bsol_error("parse", &format!("parse error: {err}")),
        "parse",
        None,
        Some("parse".to_string()),
        crate::analysis::Severity::Error,
    )
}

pub fn parse_error_diagnostic(
    source_name: &str,
    source: &str,
    err: &ParseError,
) -> SemanticDiagnostic {
    match err {
        ParseError::UnexpectedRule {
            expected,
            found,
            span,
        } => {
            let message = match expected {
                Some(rule) => format!("parse error: expected {rule:?}, found {found:?}"),
                None => format!("parse error: unexpected {found:?}"),
            };
            crate::analysis::diagnostics::make_diagnostic(
                source_name,
                source,
                *span,
                bsol_error("parse", &message),
                "parse",
                None,
                Some("parse".to_string()),
                crate::analysis::Severity::Error,
            )
        }
        ParseError::MissingPair { expected } => crate::analysis::diagnostics::make_diagnostic(
            source_name,
            source,
            crate::syntax::SpanInfo {
                start: 0,
                end: 0,
                line_col_start: (1, 1),
                line_col_end: (1, 1),
            },
            bsol_error("parse", &format!("parse error: missing {expected:?}")),
            "parse",
            None,
            Some("parse".to_string()),
            crate::analysis::Severity::Error,
        ),
        ParseError::ForbiddenImplSelfParameter { span } => {
            crate::analysis::diagnostics::make_diagnostic(
                source_name,
                source,
                *span,
                bsol_error(
                    "parse",
                    "parse error: explicit `self` parameter is not allowed in impl methods",
                ),
                "parse",
                None,
                Some("parse".to_string()),
                crate::analysis::Severity::Error,
            )
        }
    }
}

pub fn parse_recovery_diagnostic(
    source_name: &str,
    source: &str,
    span: crate::syntax::SpanInfo,
    message: &str,
) -> SemanticDiagnostic {
    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        span,
        bsol_error("parse.recovery", message),
        "parse.recovery",
        None,
        Some("parse.recovery".to_string()),
        crate::analysis::Severity::Warning,
    )
}

pub fn project_error_diagnostic(
    source_name: &str,
    source: &str,
    error: &ProjectError,
) -> SemanticDiagnostic {
    let (span, message): (crate::syntax::SpanInfo, String) = match error {
        ProjectError::ParseAt {
            line,
            message,
            start,
            end,
        } => {
            let span = if let (Some(s), Some(e)) = (start, end) {
                if *e > *s {
                    crate::syntax::SpanInfo::from_byte_range_in_source(source, *s, *e)
                } else {
                    crate::syntax::SpanInfo::whole_line_in_source(source, *line)
                }
            } else {
                crate::syntax::SpanInfo::whole_line_in_source(source, *line)
            };
            (span, message.clone())
        }
        _ => {
            let end = if source.is_empty() {
                0
            } else {
                1.min(source.len())
            };
            (
                crate::syntax::SpanInfo {
                    start: 0,
                    end,
                    line_col_start: (1, 1),
                    line_col_end: (1, 1),
                },
                error.to_string(),
            )
        }
    };

    let code = error.code().to_string();
    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        span,
        bsol_error(&code, &message),
        "project",
        None,
        Some(code),
        crate::analysis::Severity::Error,
    )
}
