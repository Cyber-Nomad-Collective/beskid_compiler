//! Recovery orchestration pipeline (strategy-driven).
//!
//! This keeps candidate generation ordered by recovery phase, which makes it easier to evolve
//! toward SOTA-style staged strategies (single-token repair, expected-token repair, then panic-mode
//! synchronization and structural heuristics).

use std::collections::HashSet;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::parser::Rule;
use crate::services::diagnostics_emit::parse_recovery_diagnostic;
use crate::syntax::SpanInfo;

use super::candidate::RepairCandidate;
use super::edit::apply_repair;
use super::{scan, policy};

/// Collect, phase-score, dedupe, and cap recovery source candidates with diagnostics.
pub(crate) fn collect_repair_candidates(
    source_name: &str,
    source: &str,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<(String, Vec<SemanticDiagnostic>)> {
    collect_repair_candidates_with_policy(source_name, source, parse_error, policy::recovery_phases())
}

pub(crate) fn collect_repair_candidates_with_policy(
    source_name: &str,
    source: &str,
    parse_error: &pest::error::Error<Rule>,
    phases: &[policy::RecoveryPhase],
) -> Vec<(String, Vec<SemanticDiagnostic>)> {
    let error_pos = scan::error_byte_pos(parse_error);
    let mut repairs = Vec::new();

    for phase in phases {
        let mut phase_repairs = (phase.strategy)(source, error_pos, parse_error);
        if phase_repairs.len() > phase.max_candidates {
            phase_repairs.truncate(phase.max_candidates);
        }
        for repair in phase_repairs {
            repairs.push(repair);
        }
    }

    repairs.sort_by(|a, b| {
        let a_cost = recovery_candidate_cost(a, error_pos);
        let b_cost = recovery_candidate_cost(b, error_pos);
        a.priority
            .cmp(&b.priority)
            .then_with(|| a_cost.cmp(&b_cost))
            .then_with(|| a.position.cmp(&b.position))
            .then_with(|| a.reason.cmp(b.reason))
    });

    let mut out: Vec<(String, Vec<SemanticDiagnostic>)> = Vec::new();
    let mut seen = HashSet::new();
    for repair in repairs {
        let Some(repaired) = apply_repair(source, &repair) else {
            continue;
        };
        if !seen.insert(repaired.clone()) {
            continue;
        }
        let diagnostics = vec![parse_recovery_diagnostic(
            source_name,
            source,
            SpanInfo::from_byte_range_in_source(source, repair.position, repair.position),
            repair.reason,
        )];
        out.push((repaired, diagnostics));
        if out.len() >= policy::MAX_RECOVERY_CANDIDATES {
            break;
        }
    }

    if out.is_empty() {
        out.push((source.to_string(), Vec::new()));
    }
    out
}

fn recovery_candidate_cost(candidate: &RepairCandidate, error_pos: usize) -> usize {
    let position_distance = candidate.position.abs_diff(error_pos);
    let edit_cost = match &candidate.kind {
        super::candidate::RepairKind::InsertStatic { text } => text.len(),
        super::candidate::RepairKind::InsertDynamic { text } => text.len(),
        super::candidate::RepairKind::Delete { len } => *len,
        super::candidate::RepairKind::Replace { len, text } => (*len).max(text.len()),
    };

    position_distance + edit_cost.min(16)
}
