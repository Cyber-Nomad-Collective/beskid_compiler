#![allow(unused_assignments)]
//! [`SemanticDiagnostic`] (miette-backed) and helpers for stable codes, spans, and severity.

use std::fmt;

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::syntax::SpanInfo;

/// Diagnostic band for rules and parse/project adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// One issue anchored in source with optional help text and machine-readable `code`.
#[derive(Error, Diagnostic, Debug, Clone)]
#[error("{message}")]
pub struct SemanticDiagnostic {
    #[source_code]
    pub src: NamedSource<String>,
    #[label("{label}")]
    pub span: SourceSpan,
    pub message: String,
    pub label: String,
    #[help]
    pub help: Option<String>,
    pub code: Option<String>,
    pub severity: Severity,
}

/// Wraps a [`SemanticDiagnostic`] so it can be stored as the root payload of an [`anyhow::Error`]
/// while remaining downcastable for CLI / tooling that want a structured [`miette::Report`].
#[derive(Debug, Clone)]
pub struct MietteReportError(SemanticDiagnostic);

impl MietteReportError {
    pub fn new(diagnostic: SemanticDiagnostic) -> Self {
        Self(diagnostic)
    }

    pub fn diagnostic(&self) -> &SemanticDiagnostic {
        &self.0
    }

    pub fn into_diagnostic(self) -> SemanticDiagnostic {
        self.0
    }
}

impl fmt::Display for MietteReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.message)
    }
}

impl std::error::Error for MietteReportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

pub fn span_to_sourcespan(span: SpanInfo) -> SourceSpan {
    let len = span.end.saturating_sub(span.start).max(1);
    SourceSpan::new(span.start.into(), len)
}

pub fn make_diagnostic(
    source_name: &str,
    source: &str,
    span: SpanInfo,
    message: impl Into<String>,
    label: impl Into<String>,
    help: Option<String>,
    code: Option<String>,
    severity: Severity,
) -> SemanticDiagnostic {
    SemanticDiagnostic {
        src: NamedSource::new(source_name, source.to_string()),
        span: span_to_sourcespan(span),
        message: message.into(),
        label: label.into(),
        help,
        code,
        severity,
    }
}

#[macro_export]
macro_rules! diag {
    ($ctx:expr, $span:expr, $code:expr, $message:expr $(, label = $label:expr)? $(, help = $help:expr)? $(, severity = $severity:expr)? ) => {{
        let label = $crate::diag!(@label $( $label )?);
        let help = $crate::diag!(@help $( $help )?);
        let severity = $crate::diag!(@severity $( $severity )?);
        let diagnostic = $crate::analysis::diagnostics::make_diagnostic(
            $ctx.source_name(),
            $ctx.source(),
            $span,
            $message,
            label,
            help,
            Some($code.to_string()),
            severity,
        );
        $ctx.emit(diagnostic);
    }};
    (@label $label:expr) => { $label };
    (@label) => { "here" };
    (@help $help:expr) => { Some($help.to_string()) };
    (@help) => { None };
    (@severity $severity:expr) => { $severity };
    (@severity) => { $crate::analysis::diagnostics::Severity::Error };
}

#[cfg(test)]
mod miette_report_error_tests {
    use super::*;

    #[test]
    fn miette_report_error_anyhow_roundtrip() {
        let diagnostic = make_diagnostic(
            "test.bd",
            "hello",
            SpanInfo {
                start: 0,
                end: 1,
                line_col_start: (1, 1),
                line_col_end: (1, 2),
            },
            "example diagnostic",
            "here",
            None,
            Some("E9999".to_string()),
            Severity::Error,
        );
        let wrapped = MietteReportError::new(diagnostic.clone());
        let anyhow_err = anyhow::Error::new(wrapped);
        let downcast = anyhow_err
            .downcast_ref::<MietteReportError>()
            .expect("MietteReportError should round-trip through anyhow");
        assert_eq!(downcast.diagnostic().message, diagnostic.message);
        assert_eq!(
            downcast.diagnostic().code.as_deref(),
            diagnostic.code.as_deref()
        );
    }
}
