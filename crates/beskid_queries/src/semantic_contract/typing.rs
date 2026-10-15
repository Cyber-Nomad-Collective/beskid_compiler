//! Focused semantic-contract implementation cluster.

mod expression;
mod managed_reference;
mod match_binding;
mod node_type;

pub(in crate::semantic_contract) use expression::{
    element_type_for_for_iterable, for_iterable_declared_array_element, integer_literal_semantic_type, local_declaration_type,
    range_iterable_bounds, semantic_type_for_binary_operands,
    semantic_type_for_expression, semantic_type_for_literal, semantic_type_for_local_path, semantic_type_for_node,
};
pub use managed_reference::specialized_call_result_managed_reference_kind;
pub(in crate::semantic_contract) use managed_reference::{
    managed_reference_kind_for_syntax_type, managed_reference_kind_tracked,
};
use match_binding::join_match_arm_type;
pub(crate) use match_binding::pattern_binding_abi_type;
pub use match_binding::pattern_binding_specialization;
pub(in crate::semantic_contract) use match_binding::{
    enum_match_pattern_binding, pattern_binding_fact, pattern_binding_fact_in_environment,
    pattern_binding_semantic_type,
};
pub(in crate::semantic_contract) use node_type::{match_arms_result_semantic_type, node_type_tracked};
