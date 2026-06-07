use std::io::{self, Write};

use anyhow::Error;
use beskid_analysis::parser::Rule;
use beskid_analysis::parsing::error::ParseError;
use beskid_analysis::services::{self, LowerResolveTypeError, SemanticDiagnosticsError};
use beskid_analysis::{MietteReportError, SemanticDiagnostic};
use miette::{Diagnostic, GraphicalReportHandler, Report};
use pest::error::Error as PestError;

/// Render a diagnostic into a string without touching stderr.
///
/// Buffering avoids interleaving with the Ratatui TUI and reduces TTY re-entrancy issues when the
/// progress UI was active moments earlier.
pub fn format_diagnostic(diagnostic: &(dyn Diagnostic + '_)) -> String {
    let mut out = String::new();
    let handler = GraphicalReportHandler::new();
    if handler.render_report(&mut out, diagnostic).is_err() {
        return diagnostic.to_string();
    }
    out
}

/// Like [`format_diagnostic`] for a [`Report`].
pub fn format_report(report: &Report) -> String {
    format_diagnostic(&**report)
}

/// Human-facing miette output written after any progress UI has been halted.
pub fn print_report(report: &Report) {
    eprint!("{}", format_report(report));
    let _ = io::stderr().flush();
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

/// Print each semantic diagnostic using miette rendering.
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
    let diagnostics = bundle.diagnostics();
    if diagnostics.is_empty() {
        return miette::miette!("semantic errors (empty diagnostic list)");
    }
    if diagnostics.len() == 1 {
        return Report::new(diagnostics[0].clone());
    }
    Report::new(diagnostics[0].clone())
}
