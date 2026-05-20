//! Diagnostic count summaries for CLI sessions.

use beskid_analysis::analysis::Severity;
use beskid_analysis::analysis::SemanticDiagnostic;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SeverityCounts {
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
}

pub fn count_severities(diagnostics: &[SemanticDiagnostic]) -> SeverityCounts {
    let mut counts = SeverityCounts::default();
    for diagnostic in diagnostics {
        match diagnostic.severity {
            Severity::Error => counts.errors += 1,
            Severity::Warning => counts.warnings += 1,
            Severity::Note => counts.notes += 1,
        }
    }
    counts
}

pub fn format_severity_summary(counts: SeverityCounts) -> String {
    if counts.errors == 0 && counts.warnings == 0 && counts.notes == 0 {
        return "no issues".to_owned();
    }
    let mut parts = Vec::new();
    if counts.errors > 0 {
        parts.push(format!("{} error(s)", counts.errors));
    }
    if counts.warnings > 0 {
        parts.push(format!("{} warning(s)", counts.warnings));
    }
    if counts.notes > 0 {
        parts.push(format!("{} note(s)", counts.notes));
    }
    parts.join(", ")
}
