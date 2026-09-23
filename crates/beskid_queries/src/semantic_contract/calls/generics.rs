//! Focused call-semantics implementation.

mod callables;
mod explicit_arguments;
mod expression_identity;
mod source_identity;
mod tracked;

pub(in crate::semantic_contract) use callables::{
    exported_generic_type_named, expression_is_lambda, function_declares_generics, generic_call_instantiation_for_node,
    generic_callable_parameters, imported_call_receiver_exists,
    imported_generic_nominal_receiver_requires_instantiation,
};
pub(in crate::semantic_contract) use explicit_arguments::{
    expected_explicit_call_argument_type, explicit_generic_type_argument_syntax,
    generic_call_uses_parameter_type_arguments, substitute_explicit_type,
    type_syntax_is_enclosing_generic_parameter_reference, type_syntax_is_generic_parameter_reference,
};
pub(in crate::semantic_contract) use expression_identity::generic_source_expression_identity;
use source_identity::generic_source_path_identity;
pub(in crate::semantic_contract) use source_identity::{
    generic_source_local_identity, generic_source_type_identity, generic_source_type_identity_with_substitutions,
    stable_declaration_identity,
};
pub(in crate::semantic_contract) use tracked::{
    generic_call_instantiation_tracked, generic_call_specialization_tracked, generic_call_template_tracked,
    generic_nominal_method_receiver_tracked,
};
