//! Focused semantic-contract implementation cluster.

mod affine;
mod affine_ownership;
mod capture_bounds;
mod closures;
mod function_values;
mod intrinsics;
mod spawn;

use super::{AstNodeKey, Db, layouts, unique_type_in_unit};

/// Find the Fiber declaration at its package-native module path `Concurrency.Fiber`.
fn fiber_declaration_in_scope(db: &dyn Db, key: AstNodeKey) -> Option<AstNodeKey> {
    layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
        .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))
}

pub(in crate::semantic_contract) use closures::{
    callable_signature_for_node, callable_signature_for_path, callable_signature_tracked, capture_storage_class,
    capture_storage_for_node, capture_storage_tracked, closure_call_target_tracked, closure_captures,
    closure_environment_for_node, closure_environment_tracked, closure_signature_for_node, closure_signature_tracked,
};
pub(in crate::semantic_contract) use function_values::{
    function_item_value_identity, function_item_value_tracked, function_value_call_tracked, function_value_callee,
    lambda_has_statement_body, lambda_source_identity, lambda_value_required_tracked, names_generic_function_item,
    untyped_lambda_parameter_identity, untyped_lambda_parameter_type,
};
pub use function_values::function_value_call_specialization;
pub(in crate::semantic_contract) use affine::{identity_spelling, move_expression_form_for_node};
pub(in crate::semantic_contract) use affine_ownership::{
    callable_affine_ownership_tracked, callable_fiber_ownership_tracked, spawn_legality_tracked,
};
pub(in crate::semantic_contract) use capture_bounds::{
    CapturesBound, call_argument_captures_bound, captures_bound, declared_parameters, direct_callee,
    field_captures_bound, with_declaration,
};
pub(in crate::semantic_contract) use intrinsics::{runtime_intrinsic_name_tracked, runtime_intrinsic_tracked};
pub(in crate::semantic_contract) use spawn::{
    inferred_spawn_handle, normalized_expression_node, spawn_entry_operand, spawn_entry_validation_tracked,
    spawn_handle_type_tracked, spawn_stack_capture, spawn_target_tracked,
};
