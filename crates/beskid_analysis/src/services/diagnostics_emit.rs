use crate::analysis::diagnostics::{SemanticDiagnostic, Severity, make_diagnostic};
use crate::parser::Rule;
use crate::parsing::diagnose::{codes, diagnose_parse_failure};
use crate::parsing::error::ParseError;
use crate::projects::ProjectError;
use crate::rule_words::rule_words;
use crate::syntax::SpanInfo;

/// A grammar error as a structured parse diagnostic (`E0101`–`E0199`), for a failed `Program` parse.
///
/// Prefer [`crate::services::parse_program_with_source_name_and_diagnostics`], which also offers
/// fixes; this entry point only describes the error.
pub fn pest_error_diagnostic(source_name: &str, source: &str, err: &pest::error::Error<Rule>) -> SemanticDiagnostic {
    diagnose_parse_failure(source_name, source, Rule::Program, err).diagnostic
}

/// A failure while building the syntax tree from a successful grammar parse.
pub fn parse_error_diagnostic(source_name: &str, source: &str, err: &ParseError) -> SemanticDiagnostic {
    let diagnostic = |span: SpanInfo, message: String, label: &str, help: Option<String>, code: &str| {
        make_diagnostic(source_name, source, span, message, label, help, Some(code.to_string()), Severity::Error)
    };
    match err {
        ParseError::UnexpectedRule { expected, found, span } => {
            let found_words = rule_words(*found).render();
            let message = match expected {
                Some(rule) => format!("expected {}, found {found_words}", rule_words(*rule).render()),
                None => format!("unexpected {found_words}"),
            };
            diagnostic(*span, message, "unexpected here", None, codes::UNEXPECTED_TREE)
        }
        ParseError::MissingPair { expected } => diagnostic(
            SpanInfo { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) },
            format!("expected {}", rule_words(*expected).render()),
            "missing here",
            None,
            codes::UNEXPECTED_TREE,
        ),
        ParseError::ForbiddenImplSelfParameter { span } => diagnostic(
            *span,
            "a method receiver is written `this`, not `self`".to_string(),
            "`self` parameter",
            Some("write `this` as the first parameter, or leave the receiver implicit".to_string()),
            codes::SELF_PARAMETER,
        ),
        ParseError::UnknownStringEscape { span, escape } => diagnostic(
            *span,
            format!("unknown escape `{escape}` in string"),
            "unknown escape",
            Some(
                "the escapes are `\\n`, `\\t`, `\\r`, `\\0`, `\\\"`, `\\\\`, `\\${` and `\\u{...}`; write `\\\\` for a backslash"
                    .to_string(),
            ),
            codes::UNKNOWN_ESCAPE,
        ),
        ParseError::InvalidUnicodeEscape { span, escape, reason } => diagnostic(
            *span,
            format!("invalid escape `{escape}`: {reason}"),
            "invalid escape",
            Some("write one to six hexadecimal digits of a Unicode scalar value, for example `\\u{1F600}`".to_string()),
            codes::INVALID_UNICODE_ESCAPE,
        ),
    }
}

pub fn project_error_diagnostic(source_name: &str, source: &str, error: &ProjectError) -> SemanticDiagnostic {
    let (span, message): (crate::syntax::SpanInfo, String) = match error {
        ProjectError::ParseAt { line, message, start, end } => {
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
            let end = if source.is_empty() { 0 } else { 1.min(source.len()) };
            (crate::syntax::SpanInfo { start: 0, end, line_col_start: (1, 1), line_col_end: (1, 1) }, error.to_string())
        }
    };

    let code = error.code().to_string();
    crate::analysis::diagnostics::make_diagnostic(
        source_name,
        source,
        span,
        message.clone(),
        "manifest errmsg",
        None,
        Some(code),
        crate::analysis::Severity::Error,
    )
}
