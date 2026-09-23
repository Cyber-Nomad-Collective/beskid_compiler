//! Public generic specialization entry points and typed Corelib value-service results.

use super::super::super::layouts::unique_assembled_type_in_module;
use super::super::super::*;
use super::*;

/// Source result of an authorized typed value service. The manifest signature describes its
/// status/out-slot transport; it is never the type of the source expression returned to callers.
pub fn specialized_corelib_value_service_result(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<GenericSubstitution> {
    let Some(CallLowering::CorelibService(service)) = call_lowering(db, key)? else { return Ok(None) };
    if beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service).is_none() {
        return Ok(None);
    }
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("corelib_value_service_result"))?;
    let call = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
        .ok_or_else(|| SemanticError::unavailable("corelib_value_service_result"))?;
    let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node else { return Ok(None) };
    let [terminal] = callee.node.path.node.segments.as_slice() else { return Ok(None) };
    let [argument] = terminal.node.type_args.as_slice() else { return Ok(None) };
    let environment =
        enclosing.substitutions.iter().map(|binding| (binding.parameter.as_ref(), binding.source_identity())).collect();
    let source = generic_source_type_identity_with_substitutions(db, key, &argument.node, &environment)?;
    let binding = GenericSubstitution::from_source("value", source.abi_type(), source);
    if binding.argument == SemanticTypeId::POINTER
        && binding.managed_reference_kind() != ManagedReferenceKind::GcManaged
    {
        return Err(SemanticError::new(
            "typed Corelib value service requires a proven managed source identity for pointer values",
        ));
    }
    Ok(Some(binding))
}

/// Materialize a call-owned specialization with its deterministic emitted declaration identity.
///
/// Keeping this conversion in the semantic layer prevents backend adapters from rebuilding
/// source identity from ABI-only facts.
pub fn generic_call_specialization_instance(
    db: &dyn Db,
    specialization: GenericCallSpecialization,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let declaration_identity = stable_declaration_identity(db, specialization.declaration)
        .ok_or_else(|| SemanticError::unavailable("generic_specialization_identity"))?;
    Ok(Some(GenericSpecializationInstance {
        declaration: specialization.declaration,
        declaration_identity,
        signature: specialization.signature,
        substitutions: specialization.substitutions,
        contract_witnesses: specialization.contract_witnesses,
    }))
}

/// Materialize an explicit generic call nested in an already-specialized declaration.
///
/// Source type arguments are recursively interpreted in the enclosing source environment, so
/// applications such as `Empty<MapEntry<TKey, TValue>>()` retain their complete nominal shape.
/// This is deliberately an internal semantic operation rather than a serialized template model:
/// code generation receives one proven concrete instance and never reconstructs source types.
pub fn generic_call_specialization_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    if let Some(instance) = contract_method_specialization(db, key, enclosing)? {
        return Ok(Some(instance));
    }
    let Some(CallLowering::Direct(declaration)) = call_lowering(db, key)? else { return Ok(None) };
    if generic_callable_parameters(db, declaration).is_none()
        && contract_parameter_declarations(db, declaration).is_empty()
    {
        return Ok(None);
    }
    specialization_for_call_in_environment(db, key, Some(enclosing)).map(Some)
}

/// Derive the concrete declaration environment and ABI shape for one direct generic call.
///
/// This is shared by call facts and module worklist construction so the latter never tries to
/// reconstruct substitutions from mangled ABI types.
pub(in crate::semantic_contract) fn generic_specialization_instance_for_call(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<GenericSpecializationInstance, SemanticError> {
    specialization_for_call_in_environment(db, key, None)
}
