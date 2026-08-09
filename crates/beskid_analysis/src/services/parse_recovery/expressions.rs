//! Expression / pattern recovery candidates (match, lambda, literals, calls).

mod calls_index_member_operator;
mod literals_lists;
mod match_lambda;
mod priorities;
mod scanner_context;

use crate::parser::Rule;

use super::{candidate::RepairCandidate, syntax_primitives};
use calls_index_member_operator::{
    control_expression_body_repairs, expression_operator_repairs, index_expression_repairs, member_access_repairs,
    paren_argument_separator_repairs,
};
use literals_lists::{
    angle_list_separator_repairs, array_literal_repairs, bracket_argument_separator_repairs, paren_expression_repairs,
    struct_field_separator_repairs, struct_literal_repairs,
};
use match_lambda::{lambda_repairs, match_repairs};
use scanner_context::recovery_insert_pos;

/// Generate expression- and pattern-oriented repairs near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let tail_pos = source.trim_end().len();
    let insert_at = if error_pos >= tail_pos
        && tail_pos > 0
        && (source[..tail_pos].ends_with('.') || source[..tail_pos].ends_with('['))
    {
        tail_pos
    } else {
        recovery_insert_pos(source, error_pos)
    };

    match_repairs(source, error_pos, insert_at, &mut candidates);
    lambda_repairs(source, error_pos, insert_at, &mut candidates);
    struct_literal_repairs(source, error_pos, insert_at, &mut candidates);
    struct_field_separator_repairs(source, error_pos, insert_at, &mut candidates);
    array_literal_repairs(source, error_pos, insert_at, &mut candidates);
    paren_expression_repairs(source, error_pos, insert_at, &mut candidates);
    paren_argument_separator_repairs(source, error_pos, insert_at, &mut candidates);
    bracket_argument_separator_repairs(source, error_pos, insert_at, &mut candidates);
    expression_operator_repairs(source, error_pos, insert_at, parse_error, &mut candidates);
    member_access_repairs(source, error_pos, insert_at, parse_error, &mut candidates);
    index_expression_repairs(source, error_pos, insert_at, &mut candidates);
    control_expression_body_repairs(source, error_pos, insert_at, parse_error, &mut candidates);
    angle_list_separator_repairs(source, error_pos, insert_at, &mut candidates);

    candidates
}
