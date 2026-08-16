use std::io::{self, Write};

use anyhow::Error;
use beskid_analysis::analysis::Severity as AnalysisSeverity;
use beskid_analysis::parser::Rule;
use beskid_analysis::parsing::error::ParseError;
use beskid_analysis::services::{self, SemanticDiagnosticsError};
use beskid_analysis::{MietteReportError, SemanticDiagnostic};
use miette::{Diagnostic, GraphicalReportHandler, Report, Severity};
use pest::error::Error as PestError;
use tracing::{error, warn};

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

fn emit_report_diagnostic_event(report: &Report) {
    match report.severity() {
        Some(Severity::Warning) => {
            warn!(
                target: "beskid.compiler",
                diagnostic_code = %report.code().as_ref().map(|value| value.to_string()).unwrap_or_else(|| "unknown".into()),
                diagnostic_phase = "compiler_reporting",
                component = "compiler",
                "compiler warning diagnostic emitted"
            );
        }
        Some(Severity::Error) => {
            error!(
                target: "beskid.compiler",
                diagnostic_code = %report.code().as_ref().map(|value| value.to_string()).unwrap_or_else(|| "unknown".into()),
                diagnostic_phase = "compiler_reporting",
                component = "compiler",
                "compiler error diagnostic emitted"
            );
        }
        _ => {}
    }
}

fn emit_semantic_diagnostic_event(diagnostic: &SemanticDiagnostic) {
    match diagnostic.severity {
        AnalysisSeverity::Warning => warn!(
            target: "beskid.compiler",
            diagnostic_code = diagnostic.code.as_deref().unwrap_or("unknown"),
            diagnostic_phase = "semantic_diagnostics",
            component = "compiler",
            "compiler warning diagnostic emitted"
        ),
        AnalysisSeverity::Error => error!(
            target: "beskid.compiler",
            diagnostic_code = diagnostic.code.as_deref().unwrap_or("unknown"),
            diagnostic_phase = "semantic_diagnostics",
            component = "compiler",
            "compiler error diagnostic emitted"
        ),
        AnalysisSeverity::Note => {}
    }
}

fn print_report_inner(report: &Report, emit_telemetry: bool) {
    if emit_telemetry {
        emit_report_diagnostic_event(report);
    }
    eprint!("{}", format_report(report));
    let _ = io::stderr().flush();
}

/// Human-facing miette output written after any progress UI has been halted.
pub fn print_report(report: &Report) {
    print_report_inner(report, true);
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
        emit_semantic_diagnostic_event(&diagnostic);
        print_report_inner(&Report::new(diagnostic), false);
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
