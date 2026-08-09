//! Canonical control-flow, signature, specialization, and ABI-type facts.

mod control_flow;
mod signatures;
mod specialization;
mod types;

pub use specialization::generic_specialization_instance;

pub(in crate::semantic_contract) use control_flow::{
    block_may_fall_through,
    control_flow_for_node,
    control_flow_tracked,
    if_may_fall_through,
    statement_may_fall_through,
    statements_may_fall_through,
};

pub(in crate::semantic_contract) use signatures::{
    call_abi_signature_for_call,
    call_abi_signature_tracked,
    item_abi_signature_tracked,
    item_signature_for_node,
    item_signature_tracked,
    signature_from_syntax,
};

pub(in crate::semantic_contract) use specialization::{
    abi_signature_from_syntax,
    binary_operand_abi_type_tracked,
    builtin_type_to_semantic,
    call_argument_abi_type_tracked,
    contextual_constant_integer,
    corelib_service_abi_signature,
    dispatch_builtin_abi_signature,
    exact_assembled_nominal_envelope,
    generic_abi_type,
    generic_parameter_reference_name,
    generic_specialization_instance_for_call,
    generic_type_name,
    integer_has_explicit_abi_suffix,
    integer_literal_fits_abi,
    integer_literal_text,
    integer_literal_u64,
    item_abi_type_from_syntax,
    type_syntax_mentions_generic_parameter,
    unsuffixed_integer_literal,
};

pub(in crate::semantic_contract) use types::{
    abi_type_for_binary_expression,
    abi_type_for_expression,
    abi_type_from_syntax,
    abi_type_tracked,
    contextual_integer_literal_abi_type_tracked,
};
