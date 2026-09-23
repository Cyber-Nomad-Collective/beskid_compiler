//! Focused ABI semantic implementation.

mod argument_abi;
mod entry_points;
mod inference;
mod instance;
mod manifest_map;

pub(in crate::semantic_contract) use argument_abi::{
    binary_operand_abi_type_tracked, call_argument_abi_type_tracked, contextual_constant_integer,
    integer_has_explicit_abi_suffix, integer_literal_fits_abi, integer_literal_text, integer_literal_u64,
    unsuffixed_integer_literal,
};
pub(in crate::semantic_contract) use entry_points::generic_specialization_instance_for_call;
pub use entry_points::{
    generic_call_specialization_in_environment, generic_call_specialization_instance,
    specialized_corelib_value_service_result,
};
use inference::specialization_for_call_in_environment;
pub use instance::generic_specialization_instance;
pub(in crate::semantic_contract) use instance::{
    abi_signature_from_syntax, exact_assembled_nominal_envelope, generic_abi_type, generic_parameter_reference_name,
    generic_type_name, item_abi_type_from_syntax, type_syntax_mentions_generic_parameter,
};
pub(in crate::semantic_contract) use manifest_map::{
    builtin_type_to_semantic, corelib_service_abi_signature, manifest_builtin_abi_signature,
};
