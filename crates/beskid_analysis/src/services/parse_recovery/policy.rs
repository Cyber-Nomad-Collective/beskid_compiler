//! Recovery policy and phase wiring for parse-recovery orchestration.
//!
//! This module centralizes candidate-generation passes in a single, staged
//! configuration (local token repair first, then structural/sync passes), following
//! the classical recovery patterns used by ANTLR-style local correction and
//! parser-framework panic-mode synchronization.

use super::candidate::RepairCandidate;
use super::{delimiters, expressions, items, separators, statements, sync};
use crate::parser::Rule;

pub(crate) const MAX_RECOVERY_CANDIDATES: usize = 24;

/// Sources up to this size get the full candidate budget.
pub(crate) const FULL_BUDGET_SOURCE_BYTES: usize = 48 * 1024;
/// Fewest candidates a source of any size gets.
pub(crate) const MIN_RECOVERY_CANDIDATES: usize = 2;

/// Repair candidates tried for a source of `source_len` bytes. Every candidate is a parse of
/// the whole file, so the budget shrinks with the file: a 48 KiB source gets all
/// [`MAX_RECOVERY_CANDIDATES`], a 96 KiB source half of them, down to
/// [`MIN_RECOVERY_CANDIDATES`]. Recovery of a large broken file then costs a bounded number of
/// parses instead of seconds per keystroke in an editor.
pub(crate) fn candidate_budget(source_len: usize) -> usize {
    if source_len <= FULL_BUDGET_SOURCE_BYTES {
        return MAX_RECOVERY_CANDIDATES;
    }
    (MAX_RECOVERY_CANDIDATES * FULL_BUDGET_SOURCE_BYTES / source_len).max(MIN_RECOVERY_CANDIDATES)
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn the_candidate_budget_shrinks_with_the_source_size() {
        assert_eq!(candidate_budget(0), MAX_RECOVERY_CANDIDATES);
        assert_eq!(candidate_budget(FULL_BUDGET_SOURCE_BYTES), MAX_RECOVERY_CANDIDATES);
        assert_eq!(candidate_budget(2 * FULL_BUDGET_SOURCE_BYTES), MAX_RECOVERY_CANDIDATES / 2);
        assert_eq!(candidate_budget(1 << 30), MIN_RECOVERY_CANDIDATES);
    }
}

type RecoveryStrategy = fn(&str, usize, &pest::error::Error<Rule>) -> Vec<RepairCandidate>;

#[derive(Copy, Clone, Debug)]
pub(crate) struct RecoveryPhase {
    pub(crate) strategy: RecoveryStrategy,
    pub(crate) max_candidates: usize,
}

const RECOVERY_PHASES: &[RecoveryPhase] = &[
    RecoveryPhase { strategy: sync::repairs, max_candidates: 16 },
    RecoveryPhase { strategy: delimiters::repairs, max_candidates: 16 },
    RecoveryPhase { strategy: statements::repairs, max_candidates: 16 },
    RecoveryPhase { strategy: items::repairs, max_candidates: 16 },
    RecoveryPhase { strategy: expressions::repairs, max_candidates: 20 },
    RecoveryPhase { strategy: separators::repairs, max_candidates: 16 },
];

pub(crate) fn recovery_phases() -> &'static [RecoveryPhase] {
    RECOVERY_PHASES
}
