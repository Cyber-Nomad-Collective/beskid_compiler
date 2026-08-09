//! Canonical call-semantics implementation clusters.

mod casts;
mod facts;
mod generics;
mod resolution;

pub use resolution::extern_contract_import_for_declaration;

pub(in crate::semantic_contract) use casts::{
    abi_semantic_type,
    canonical_intrinsic_parameter_type,
    cast_intents_for_node,
    cast_intents_tracked,
    expected_cast_type,
    expression_fact_target,
    is_transparent_binary_operand_path,
    primitive_integer,
    primitive_numeric,
};

pub(in crate::semantic_contract) use facts::{
    call_arguments_tracked,
    call_lowering_tracked,
    canonical_result_definition_for_type,
    canonical_result_variant,
    for_iterator_fact_tracked,
    primitive_numeric_conversion_tracked,
    range_for_fact_tracked,
    result_type_parts,
    same_type_syntax,
    try_expression_fact_for_node,
    try_expression_fact_tracked,
    try_operand_parameter_declaration,
};

pub(in crate::semantic_contract) use generics::{
    explicit_generic_type_argument_syntax,
    exported_generic_type_named,
    expression_is_lambda,
    function_declares_generics,
    generic_call_instantiation_for_node,
    generic_call_instantiation_tracked,
    generic_call_specialization_tracked,
    generic_call_template_tracked,
    generic_call_uses_parameter_type_arguments,
    imported_call_receiver_exists,
    imported_generic_nominal_receiver_requires_instantiation,
    type_syntax_is_generic_parameter_reference,
};

pub(in crate::semantic_contract) use resolution::{
    call_lowering_for_node,
    canonical_runtime_intrinsic_scope,
    corelib_service_for,
    flatten_member_as_path_declaration,
    method_declaration_for_member_receiver,
    nominal_local_member_receiver,
    nominal_member_receiver_tracked,
    resolve_local_extern_contract_method,
    unique_nominal_method_declaration,
};
