//! The miette drawing of [`SemanticDiagnostic`] and [`MietteReportError`], the wrapper that keeps a
//! diagnostic downcastable inside an [`anyhow::Error`].

use std::fmt;

use miette::{Diagnostic, LabeledSpan, SourceCode};

use super::model::line_column;
use super::semantic::{SemanticDiagnostic, Severity};

/// The CLI drawing: the primary label, every related location in the primary source as a
/// secondary label, and help. Related locations in other files and the fix titles are listed in
/// the help block, since miette draws one source per report.
impl Diagnostic for SemanticDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.code.as_ref().map(|code| Box::new(code) as Box<dyn fmt::Display>)
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
            Severity::Note => miette::Severity::Advice,
        })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let mut lines: Vec<String> = self.help.iter().cloned().collect();
        for related in self.related.iter().filter(|related| related.source.is_some()) {
            let text = related.file_text(self.source_text());
            let (line, column) = line_column(text, related.span.offset());
            lines.push(format!("{}: {}:{line}:{column}", related.label, related.file_name(self.file_name())));
        }
        for fix in &self.fixes {
            lines.push(format!("fix: {}", fix.editor_title()));
        }
        (!lines.is_empty()).then(|| Box::new(lines.join("\n")) as Box<dyn fmt::Display>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let primary = LabeledSpan::new_primary_with_span(Some(self.label.clone()), self.span);
        let related = self
            .related
            .iter()
            .filter(|related| related.source.is_none())
            .map(|related| LabeledSpan::new_with_span(Some(related.label.clone()), related.span));
        Some(Box::new(std::iter::once(primary).chain(related)))
    }
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

#[cfg(test)]
mod miette_report_error_tests {
    use super::*;
    use crate::analysis::diagnostics::make_diagnostic;
    use crate::syntax::SpanInfo;

    #[test]
    fn miette_report_error_anyhow_roundtrip() {
        let diagnostic = make_diagnostic(
            "test.bd",
            "hello",
            SpanInfo { start: 0, end: 1, line_col_start: (1, 1), line_col_end: (1, 2) },
            "example diagnostic",
            "here",
            None,
            Some("E9999".to_string()),
            Severity::Error,
        );
        let wrapped = MietteReportError::new(diagnostic.clone());
        let anyhow_err = anyhow::Error::new(wrapped);
        let downcast =
            anyhow_err.downcast_ref::<MietteReportError>().expect("MietteReportError should round-trip through anyhow");
        assert_eq!(downcast.diagnostic().message, diagnostic.message);
        assert_eq!(downcast.diagnostic().code.as_deref(), diagnostic.code.as_deref());
    }
}
