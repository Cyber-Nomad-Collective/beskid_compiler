//! Focused semantic-contract implementation cluster.

mod closures;
mod fiber_ownership;
mod intrinsics;
mod spawn;

pub(in crate::semantic_contract) use closures::{
    callable_signature_for_node, callable_signature_for_path, callable_signature_tracked, capture_storage_class,
    capture_storage_for_node, capture_storage_tracked, closure_call_target_tracked, closure_captures,
    closure_environment_for_node, closure_environment_tracked, closure_signature_for_node, closure_signature_tracked,
};
pub(in crate::semantic_contract) use fiber_ownership::{callable_fiber_ownership_tracked, spawn_legality_tracked};
pub(in crate::semantic_contract) use intrinsics::{runtime_intrinsic_name_tracked, runtime_intrinsic_tracked};
pub(in crate::semantic_contract) use spawn::{
    inferred_spawn_handle, normalized_expression_node, spawn_entry_operand, spawn_entry_validation_tracked,
    spawn_handle_type_tracked, spawn_stack_capture, spawn_target_tracked,
};
