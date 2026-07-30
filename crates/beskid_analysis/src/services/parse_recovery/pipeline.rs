//! Recovery orchestration helpers shared across parse entrypoints.

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::parser::Rule;

use super::collect_repair_candidates;

/// Try repair candidates from a parse failure and return the first candidate that reparses.
pub(crate) fn recover_with_repair_candidates<T, F, E>(
    source_name: &str,
    source: &str,
    parse_error: &pest::error::Error<Rule>,
    mut parse_with_candidate: F,
) -> Option<(T, Vec<SemanticDiagnostic>, bool)>
where
    F: FnMut(&str) -> Result<T, E>,
{
    for (candidate_source, mut diagnostics) in collect_repair_candidates(source_name, source, parse_error) {
        let Ok(result) = parse_with_candidate(&candidate_source) else {
            continue;
        };

        let recovered = candidate_source != source;
        if !recovered {
            diagnostics.clear();
        }
        return Some((result, diagnostics, recovered));
    }

    None
}
