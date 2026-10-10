//! Focused semantic-contract implementation cluster.

mod closures;
mod fiber_ownership;
mod function_values;
mod intrinsics;
mod spawn;

use super::{AstNodeKey, Db, layouts, unique_type_in_unit};

/// Find the Fiber declaration through the requesting unit's module namespace.
/// Corelib shards use `Concurrency.Fiber`; implicit-Std Apps use `Std.Concurrency.Fiber`.
fn fiber_declaration_in_scope(db: &dyn Db, key: AstNodeKey) -> Option<AstNodeKey> {
    layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
        .or_else(|| {
            layouts::unique_assembled_type_in_module(
                db,
                key,
                &["Std".into(), "Concurrency".into(), "Fiber".into()],
                "Fiber",
                1,
            )
        })
        .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))
}

pub(in crate::semantic_contract) use closures::{
    callable_signature_for_node, callable_signature_for_path, callable_signature_tracked, capture_storage_class,
    capture_storage_for_node, capture_storage_tracked, closure_call_target_tracked, closure_captures,
    closure_environment_for_node, closure_environment_tracked, closure_signature_for_node, closure_signature_tracked,
};
pub(in crate::semantic_contract) use function_values::{
    function_value_call_tracked, function_value_declaration, lambda_expected_signature, lambda_has_statement_body,
    lambda_value_required_tracked,
    untyped_lambda_parameter_type,
};
pub use function_values::function_value_call_specialization;
pub(in crate::semantic_contract) use fiber_ownership::{callable_fiber_ownership_tracked, spawn_legality_tracked};
pub(in crate::semantic_contract) use intrinsics::{runtime_intrinsic_name_tracked, runtime_intrinsic_tracked};
pub(in crate::semantic_contract) use spawn::{
    inferred_spawn_handle, normalized_expression_node, spawn_entry_operand, spawn_entry_validation_tracked,
    spawn_handle_type_tracked, spawn_stack_capture, spawn_target_tracked,
};
