//! Thin wrappers for pre-normalize type queries (delegates to [`TypeChecker`] precheck APIs).

use std::collections::{HashMap, HashSet};

use crate::syntax::{Expression, Program};
use crate::resolve::Resolution;
use crate::syntax::{SpanInfo, Spanned};
use crate::types::TypeChecker;
use crate::types::checker::precheck::precheck_checker;

pub use crate::types::checker::TryDesugarTarget;

pub fn try_desugar_target_for_operand(
    resolution: &Resolution,
    programs: &[&Spanned<Program>],
    operand: &Spanned<Expression>,
) -> Option<TryDesugarTarget> {
    let mut checker = precheck_checker(resolution, programs);
    checker.try_desugar_target_for_operand(operand)
}

/// Spans of `?` operands that are not a `Result`-shaped enum (semantic stage 7 / early IDE).
pub fn invalid_try_expression_spans(resolution: &Resolution, entry: &Spanned<Program>) -> Vec<SpanInfo> {
    TypeChecker::invalid_try_expression_spans(resolution, entry)
}

/// Map try-expression span → desugar metadata (computed before in-place normalization).
pub fn try_desugar_targets_for_program(
    resolution: &Resolution,
    entry: &Spanned<Program>,
    dependency_programs: &[&Spanned<Program>],
) -> HashMap<SpanInfo, TryDesugarTarget> {
    TypeChecker::try_desugar_targets_for_program(resolution, entry, dependency_programs)
}

/// Map for-statement span → true when the iterable type is `T[]` (computed before normalization).
pub fn collect_array_for_spans(
    resolution: &Resolution,
    entry: &Spanned<Program>,
    dependency_programs: &[&Spanned<Program>],
) -> HashSet<SpanInfo> {
    TypeChecker::collect_array_for_spans(resolution, entry, dependency_programs)
}
