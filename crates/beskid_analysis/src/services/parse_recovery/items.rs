//! Item / signature stub recovery for incomplete top-level constructs.

mod bodies_contracts;
mod declarations;
mod fields;
mod priorities;
mod scanner_context;

use crate::parser::Rule;

use super::{candidate::RepairCandidate, syntax_primitives};
use bodies_contracts::{
    contract_method_semicolon_repairs, empty_stub_body_repairs, missing_function_body_repairs,
    unclosed_item_brace_repairs,
};
use declarations::use_mod_semicolon_repairs;
use fields::type_and_enum_field_list_repairs;

/// Generate item-boundary repairs (closers / trailing `;`) near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let mut candidates = Vec::new();
    candidates.extend(unclosed_item_brace_repairs(source, error_pos));
    candidates.extend(type_and_enum_field_list_repairs(source, error_pos));
    candidates.extend(use_mod_semicolon_repairs(source, error_pos));
    candidates.extend(contract_method_semicolon_repairs(source, error_pos));
    candidates.extend(missing_function_body_repairs(source, error_pos, parse_error));
    candidates.extend(empty_stub_body_repairs(source, error_pos, parse_error));
    candidates
}
