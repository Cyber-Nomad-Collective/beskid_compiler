use anyhow::Error;
use beskid_analysis::parser::Rule;
use beskid_analysis::parsing::error::ParseError;
use beskid_analysis::services::{self, LowerResolveTypeError, SemanticDiagnosticsError};
use beskid_analysis::{MietteReportError, SemanticDiagnostic};
use miette::Report;
use pest::error::Error as PestError;

/// Human-facing miette output (uses `Display`, not `Debug`, so fancy rendering applies).
pub fn print_report(report: &Report) {
    eprintln!("{report}");
}

/// Pretty-print a Pest grammar error for `source` labeled as `file`.
pub fn print_pretty_pest_error(file: &str, source: &str, err: &PestError<Rule>) {
    let diagnostic = services::pest_error_diagnostic(file, source, err);
    print_report(&Report::new(diagnostic));
}

/// Pretty-print a structured parse error for `source` labeled as `file`.
pub fn print_pretty_parse_error(file: &str, source: &str, err: &ParseError) {
    let diagnostic = services::parse_error_diagnostic(file, source, err);
    print_report(&Report::new(diagnostic));
}

/// Print each semantic diagnostic on its own line using miette rendering.
pub fn print_semantic_diagnostics(diagnostics: impl IntoIterator<Item = SemanticDiagnostic>) {
    for diagnostic in diagnostics {
        print_report(&Report::new(diagnostic));
    }
}

/// Build a single [`Report`] for anything the compiler surfaced through `anyhow` while
/// preserving structured diagnostics when present.
pub fn report_from_anyhow(err: &Error) -> Report {
    if let Some(wrapped) = err.downcast_ref::<MietteReportError>() {
        return Report::new(wrapped.diagnostic().clone());
    }
    if let Some(bundle) = err.downcast_ref::<SemanticDiagnosticsError>() {
        return semantic_diagnostics_bundle_report(bundle);
    }
    if let Some(lower) = err.downcast_ref::<LowerResolveTypeError>() {
        return miette::miette!("{lower}");
    }
    miette::miette!("{err:#}")
}

fn semantic_diagnostics_bundle_report(bundle: &SemanticDiagnosticsError) -> Report {
    if bundle.diagnostics().is_empty() {
        return miette::miette!("semantic errors (empty diagnostic list)");
    }
    Report::new(bundle.clone())
}
