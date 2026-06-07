use std::error::Error;
use std::fmt;

use miette;

use crate::analysis::SemanticDiagnostic;
use crate::analysis::rules::staged::SemanticPipelineRule;
use crate::syntax::Program;
use beskid_pipeline::PipelineObserver;

/// Run built-in [`Rule`](crate::analysis::Rule) passes and collect [`SemanticDiagnostic`] list.
pub fn semantic_rule_diagnostics_for_program(
    program: &Program,
    source_name: impl Into<String>,
    source: &str,
    options: crate::AnalysisOptions,
) -> Vec<SemanticDiagnostic> {
    semantic_rule_diagnostics_for_program_with_pipeline(program, source_name, source, options, None)
}

/// Like [`semantic_rule_diagnostics_for_program`], forwarding nested pipeline sub-phases when set.
pub fn semantic_rule_diagnostics_for_program_with_pipeline(
    program: &Program,
    source_name: impl Into<String>,
    source: &str,
    options: crate::AnalysisOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Vec<SemanticDiagnostic> {
    let mut ctx = crate::analysis::rules::RuleContext::new(source_name, source, options);
    SemanticPipelineRule.run_stages(&mut ctx, program, pipeline);
    ctx.diagnostics
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
        let count = self.diagnostics.len();
        if count == 0 {
            return write!(f, "semantic errors (empty diagnostic list)");
        }
        if count == 1 {
            return write!(f, "{}", self.diagnostics[0].message);
        }
        write!(
            f,
            "{count} semantic errors (first: {})",
            self.diagnostics[0].message
        )
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
