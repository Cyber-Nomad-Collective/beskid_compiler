//! Canonical semantic layout implementation.

#[cfg(test)]
mod scalar_payload_tests;

use super::super::*;
use super::explicit_local_declaration_type;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

mod candidate_path;
mod constructor;
mod layout;
mod match_materialize;
mod scrutinee;
mod source_identity;

pub(in crate::semantic_contract) use candidate_path::{
    aggregate_shape_from_applied_type, contextual_enum_constructor_type_path, enum_field_layout,
    enum_layout_from_definition, enum_layout_substitutions, instantiated_enum_layout_for_path,
};
pub use constructor::enum_constructor_specialization;
pub(in crate::semantic_contract) use constructor::{enum_constructor_template_tracked, enum_constructor_tracked};
pub(in crate::semantic_contract) use layout::enum_layout_tracked;
pub use match_materialize::enum_match_specialization;
pub(in crate::semantic_contract) use match_materialize::enum_match_tracked;
pub(in crate::semantic_contract) use scrutinee::{enum_match_scrutinee_layout, enum_pattern_targets_declaration};
use scrutinee::{enum_match_scrutinee_layout_in_environment, enum_match_source_environment};
pub(in crate::semantic_contract) use source_identity::enum_layout_for_source_identity;
use source_identity::{enum_layout_for_direct_call_result, instantiated_enum_layout_for_path_in_environment};
