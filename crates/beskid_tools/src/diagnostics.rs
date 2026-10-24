use std::io::{self, Write};
use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::Error;
use beskid_analysis::analysis::Severity as AnalysisSeverity;
use beskid_analysis::parser::Rule;
use beskid_analysis::parsing::error::ParseError;
use beskid_analysis::services::{self, SemanticDiagnosticsError};
use beskid_analysis::analysis::diagnostics::json;
use beskid_analysis::analysis::diagnostics::source_map;
use beskid_analysis::SemanticDiagnostic;
use miette::{Diagnostic, GraphicalReportHandler, Report, Severity};
use pest::error::Error as PestError;
use tracing::{error, warn};

/// How diagnostics are written to stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorFormat {
    /// The miette drawing.
    #[default]
    Human,
    /// One JSON object per line (`beskid_analysis::analysis::diagnostics::json`).
    Json,
}

static ERROR_FORMAT: AtomicU8 = AtomicU8::new(0);

/// Select the diagnostic format for the rest of this process (`--error-format`).
pub fn set_error_format(format: ErrorFormat) {
    ERROR_FORMAT.store(format as u8, Ordering::Relaxed);
}

pub fn error_format() -> ErrorFormat {
    if ERROR_FORMAT.load(Ordering::Relaxed) == ErrorFormat::Json as u8 { ErrorFormat::Json } else { ErrorFormat::Human }
}

/// One `--error-format=json` line for `diagnostic`.
pub fn diagnostic_json_line(diagnostic: &SemanticDiagnostic) -> String {
    let rendered = format_diagnostic(diagnostic);
    json::diagnostic_json(diagnostic, Some(&rendered)).to_string()
}

/// `--error-format=json` lines for a report: one per structured diagnostic it holds, or one
/// unstructured line.
pub fn report_json_lines(report: &Report) -> Vec<String> {
    if let Some(diagnostic) = report.downcast_ref::<SemanticDiagnostic>() {
        return vec![diagnostic_json_line(diagnostic)];
    }
    if let Some(bundle) = report.downcast_ref::<SemanticDiagnosticsError>() {
        return bundle.diagnostics().iter().map(diagnostic_json_line).collect();
    }
    vec![json::message_json(&report.to_string(), Some(&format_report(report))).to_string()]
}

/// Write `diagnostic` to stderr in the selected format.
pub fn eprint_diagnostic(diagnostic: &SemanticDiagnostic) {
    match error_format() {
        ErrorFormat::Human => eprint!("{}", format_diagnostic(diagnostic)),
        ErrorFormat::Json => eprintln!("{}", diagnostic_json_line(diagnostic)),
    }
}

/// Render a diagnostic into a string without touching stderr.
///
/// Buffering keeps report rendering separate from stderr emission and line-oriented progress
/// output.
pub fn format_diagnostic(diagnostic: &(dyn Diagnostic + '_)) -> String {
    let mut out = String::new();
    let handler = GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor());
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
    match error_format() {
        ErrorFormat::Human => eprint!("{}", format_report(report)),
        ErrorFormat::Json => {
            for line in report_json_lines(report) {
                eprintln!("{line}");
            }
        }
    }
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

/// Map `diagnostics` to their authored sources (never a copy under `obj/beskid/`) and drop
/// repeated causes, keeping the first report.
pub fn authored_diagnostics(diagnostics: impl IntoIterator<Item = SemanticDiagnostic>) -> Vec<SemanticDiagnostic> {
    source_map::map_and_dedupe(diagnostics.into_iter().collect())
}

/// Print each semantic diagnostic using miette rendering.
pub fn print_semantic_diagnostics(diagnostics: impl IntoIterator<Item = SemanticDiagnostic>) {
    for diagnostic in authored_diagnostics(diagnostics) {
        emit_semantic_diagnostic_event(&diagnostic);
        print_report_inner(&Report::new(diagnostic), false);
    }
}

/// Build a single [`Report`] for anything the compiler surfaced through `anyhow` while
/// preserving structured diagnostics found anywhere in the error chain (a build wraps the gate's
/// diagnostics in context such as the AOT summary line).
pub fn report_from_anyhow(err: &Error) -> Report {
    let Some(diagnostics) = services::structured_diagnostics(err) else {
        return miette::miette!("{err:#}");
    };
    let mut diagnostics = authored_diagnostics(diagnostics);
    match diagnostics.len() {
        0 => miette::miette!("{err:#}"),
        1 => Report::new(diagnostics.remove(0)),
        // The bundle renders its first diagnostic and every other one as `related`, so a FAIL line
        // shows each finding of a legality pass with its own source excerpt.
        _ => Report::new(SemanticDiagnosticsError::from_diagnostics(diagnostics)),
    }
}

#[cfg(test)]
mod error_format_tests {
    use super::*;
    use beskid_analysis::analysis::diagnostics::make_diagnostic;
    use beskid_analysis::syntax::SpanInfo;

    fn diagnostic(code: &str) -> SemanticDiagnostic {
        make_diagnostic(
            "Src/Main.bd",
            "i32 Main() { return x; }",
            SpanInfo { start: 20, end: 21, line_col_start: (1, 21), line_col_end: (1, 22) },
            "Unknown value `x`.",
            "not found",
            None,
            Some(code.into()),
            AnalysisSeverity::Error,
        )
    }

    #[test]
    fn json_lines_cover_single_bundled_and_unstructured_reports() {
        let single = report_json_lines(&Report::new(diagnostic("E1101")));
        assert_eq!(single.len(), 1);
        let value: serde_json::Value = serde_json::from_str(&single[0]).expect("json line");
        assert_eq!(value["code"]["code"], "E1101");
        assert!(value["rendered"].as_str().is_some_and(|rendered| rendered.contains("Unknown value")));

        let bundle = SemanticDiagnosticsError::from_diagnostics(vec![diagnostic("E1101"), diagnostic("E1201")]);
        assert_eq!(report_json_lines(&Report::new(bundle)).len(), 2);

        let unstructured = report_json_lines(&miette::miette!("cannot open the manifest"));
        let value: serde_json::Value = serde_json::from_str(&unstructured[0]).expect("json line");
        assert_eq!(value["message"], "cannot open the manifest");
        assert_eq!(value["spans"], serde_json::json!([]));
    }

    #[test]
    fn error_format_defaults_to_human_and_switches_to_json() {
        assert_eq!(ErrorFormat::default(), ErrorFormat::Human);
        set_error_format(ErrorFormat::Json);
        assert_eq!(error_format(), ErrorFormat::Json);
        set_error_format(ErrorFormat::Human);
        assert_eq!(error_format(), ErrorFormat::Human);
    }

    #[test]
    fn structured_diagnostics_are_found_below_context_layers() {
        let error = anyhow::Error::new(beskid_analysis::MietteReportError::new(diagnostic("E1101")))
            .context("type checking failed")
            .context("AOT build failed: 1 error");
        let report = report_from_anyhow(&error);
        let found = report.downcast_ref::<SemanticDiagnostic>().expect("the coded diagnostic, not the summary");
        assert_eq!(found.code.as_deref(), Some("E1101"));

        let bundle = anyhow::Error::new(SemanticDiagnosticsError::from_diagnostics(vec![
            diagnostic("E1101"),
            diagnostic("E1201"),
        ]))
        .context("AOT build failed");
        let report = report_from_anyhow(&bundle);
        assert_eq!(report.downcast_ref::<SemanticDiagnosticsError>().map(|bundle| bundle.diagnostics().len()), Some(2));

        let plain = report_from_anyhow(&anyhow::anyhow!("cannot open").context("AOT build failed"));
        assert!(plain.to_string().contains("AOT build failed"));
    }
}
