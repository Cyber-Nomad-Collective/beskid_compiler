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
