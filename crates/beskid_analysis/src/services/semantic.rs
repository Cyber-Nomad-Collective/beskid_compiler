use std::error::Error;
use std::fmt;

use miette;

use crate::analysis::SemanticDiagnostic;
use crate::syntax::Program;
use crate::{AnalysisOptions, builtin_rules, run_rules};

/// Run built-in [`Rule`](crate::analysis::Rule) passes and collect [`SemanticDiagnostic`] list.
pub fn semantic_rule_diagnostics_for_program(
    program: &Program,
    source_name: impl Into<String>,
    source: &str,
    options: AnalysisOptions,
) -> Vec<SemanticDiagnostic> {
    run_rules(
        program,
        source_name.into(),
        source,
        &builtin_rules(),
        options,
    )
    .diagnostics
}

/// Carrier for fatal semantic failures (implements [`miette::Diagnostic`] for reporting).
#[derive(Debug, Clone)]
pub struct SemanticDiagnosticsError {
    diagnostics: Vec<SemanticDiagnostic>,
}

impl SemanticDiagnosticsError {
    pub fn from_diagnostics(diagnostics: Vec<SemanticDiagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn diagnostics(&self) -> &[SemanticDiagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for SemanticDiagnosticsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", miette::Report::new(self.clone()))
    }
}

impl Error for SemanticDiagnosticsError {}

impl miette::Diagnostic for SemanticDiagnosticsError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.diagnostics.first().and_then(miette::Diagnostic::code)
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.diagnostics
            .first()
            .and_then(miette::Diagnostic::severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.diagnostics.first().and_then(miette::Diagnostic::help)
    }

    fn url<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.diagnostics.first().and_then(miette::Diagnostic::url)
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.diagnostics
            .first()
            .and_then(|d| miette::Diagnostic::source_code(d))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.diagnostics
            .first()
            .and_then(miette::Diagnostic::labels)
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        if self.diagnostics.len() <= 1 {
            return None;
        }
        Some(Box::new(
            self.diagnostics[1..]
                .iter()
                .map(|d| d as &dyn miette::Diagnostic),
        ))
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        self.diagnostics
            .first()
            .and_then(|d| miette::Diagnostic::diagnostic_source(d))
    }
}

pub fn require_no_semantic_errors(
    diagnostics: &[SemanticDiagnostic],
) -> std::result::Result<(), SemanticDiagnosticsError> {
    let has_errors = diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.severity, crate::analysis::Severity::Error));
    if has_errors {
        return Err(SemanticDiagnosticsError::from_diagnostics(
            diagnostics.to_vec(),
        ));
    }
    Ok(())
}
