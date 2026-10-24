//! Recovery orchestration helpers shared across parse entrypoints.

use crate::parser::Rule;

use super::candidate::RepairCandidate;
use super::collect_repair_candidates;

/// Try repair candidates from a parse failure and return the first result that reparses, with
/// the repair that produced it (`None` when the source itself parsed).
pub(crate) fn recover_with_repair_candidates<T, F, E>(
    source: &str,
    parse_error: &pest::error::Error<Rule>,
    mut parse_with_candidate: F,
) -> Option<(T, Option<RepairCandidate>)>
where
    F: FnMut(&str) -> Result<T, E>,
{
    for (candidate_source, repair) in collect_repair_candidates(source, parse_error) {
        let Ok(result) = parse_with_candidate(&candidate_source) else {
            continue;
        };
        let repair = if candidate_source == source { None } else { repair };
        return Some((result, repair));
    }

    None
}
