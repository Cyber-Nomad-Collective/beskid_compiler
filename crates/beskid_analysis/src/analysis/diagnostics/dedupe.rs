//! Drop diagnostics that report the same cause twice.

use std::collections::HashSet;

use super::SemanticDiagnostic;

/// Keep the first diagnostic for each (code, file, span); later reports of the same key are
/// dropped. Callers dedupe after [`super::source_map`] mapped spans to the authored source, so a
/// cause reported against a materialized copy and against the authored file counts once, and the
/// first phase that reported it wins.
pub fn dedupe_diagnostics(diagnostics: Vec<SemanticDiagnostic>) -> Vec<SemanticDiagnostic> {
    let mut seen: HashSet<(Option<String>, String, usize, usize)> = HashSet::new();
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            seen.insert((
                diagnostic.code.clone(),
                diagnostic.file_name().to_string(),
                diagnostic.span.offset(),
                diagnostic.span.len(),
            ))
        })
        .collect()
}

/// Whether `covered` reports a cause one of `reported` already reports (same code, file, span).
pub fn is_covered_by(covered: &SemanticDiagnostic, reported: &[SemanticDiagnostic]) -> bool {
    reported.iter().any(|diagnostic| {
        diagnostic.code == covered.code
            && diagnostic.file_name() == covered.file_name()
            && diagnostic.span == covered.span
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diagnostics::{Severity, make_diagnostic};
    use crate::syntax::SpanInfo;

    fn at(file: &str, start: usize, code: &str, message: &str) -> SemanticDiagnostic {
        make_diagnostic(
            file,
            "i32 Main() { return 0; }",
            SpanInfo { start, end: start + 3, line_col_start: (1, 1), line_col_end: (1, 1) },
            message,
            "here",
            None,
            Some(code.to_string()),
            Severity::Error,
        )
    }

    #[test]
    fn first_report_of_a_code_file_and_span_wins() {
        let kept = dedupe_diagnostics(vec![
            at("Main.bd", 0, "E1201", "first phase"),
            at("Main.bd", 0, "E1201", "second phase words it differently"),
            at("Main.bd", 4, "E1201", "other span"),
            at("Other.bd", 0, "E1201", "other file"),
            at("Main.bd", 0, "E1101", "other code"),
        ]);
        let messages = kept.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>();
        assert_eq!(messages, ["first phase", "other span", "other file", "other code"]);
    }

    #[test]
    fn coverage_uses_the_dedupe_key() {
        let reported = vec![at("Main.bd", 0, "E1201", "a")];
        assert!(is_covered_by(&at("Main.bd", 0, "E1201", "b"), &reported));
        assert!(!is_covered_by(&at("Main.bd", 4, "E1201", "a"), &reported));
    }
}
