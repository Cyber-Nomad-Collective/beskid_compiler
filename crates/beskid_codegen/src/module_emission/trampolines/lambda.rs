use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee, LocalSlotId, ManagedReferenceFact, ParameterSlot};
use beskid_queries::{
    GenericSpecializationInstance, ManagedReferenceKind, child_nodes, closure_environment, closure_signature,
    event_handler_lambda_for_local, generic_specialization_identity, local_slot, managed_reference_kind, node_kind,
    specialized_closure_signature,
};
use cranelift_codegen::ir::AbiParam;
use cranelift_codegen::isa::TargetIsa;

use super::types::LambdaTrampoline;
use crate::CodegenInput;
use crate::isle_adapter::mappings::{map_signature_type, signature_for_item};
use crate::module_emission::contracts::{SyntaxModuleEmissionError, emission_verification};
use crate::module_emission::items::ResolvedSyntaxModuleItem;

/// Resolve trampoline entries for every freestanding [`LambdaExpression`] in the syntax tree.
///
/// Capture-free lambdas emit a simple entry function. Capturing lambdas require
/// generation-safe allocate/store/root authority before the entry is emitted.
pub(in crate::module_emission) fn resolve_lambda_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    _symbols: &HashMap<DirectCallee, String>,
) -> Result<Vec<LambdaTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut lambdas = Vec::new();
    let mut visited = HashSet::new();
    let mut specialized = Vec::new();
    for item in items {
        match &item.specialization {
            None => collect_lambda_nodes(db, item.key, &mut visited, &mut lambdas),
            // A lambda in a generic body is emitted once per enclosing instance, in its environment.
            Some(instance) => {
                let mut nested = Vec::new();
                collect_lambda_nodes(db, item.key, &mut HashSet::new(), &mut nested);
                for lambda in nested {
                    // Event handlers keep their checked, environment-free entry.
                    if matches!(event_handler_lambda_for_local(db, lambda), Ok(Some(_))) {
                        if visited.insert(lambda) {
                            lambdas.push(lambda);
                        }
                    } else {
                        specialized.push((lambda, instance, item.symbol.as_str()));
                    }
                }
            }
        }
    }
    let mut trampolines = Vec::new();
    for (lambda, instance, owner_symbol) in specialized {
        trampolines.push(resolve_specialized_lambda_trampoline(input, isa, lambda, instance, owner_symbol)?);
    }
    for lambda in lambdas {
        let event_handler =
            event_handler_lambda_for_local(db, lambda).map_err(|error| emission_verification(error.to_string()))?;
        let is_event_handler = event_handler.is_some();
        let (lambda_body, callable) = if let Some(handler) = event_handler {
            (handler.body, handler.signature)
        } else {
            let Some(lambda_sig) =
                closure_signature(db, lambda).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            (lambda_sig.body, lambda_sig.callable)
        };
        let Some(mut signature) = signature_for_item(isa, callable.clone()) else {
            continue;
        };
        let Some(environment) =
            closure_environment(db, lambda).map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        if environment.parameters.len() != callable.parameters.len() {
            return Err(emission_verification("lambda parameter facts do not match its callable signature"));
        }
        let parameters = environment
            .parameters
            .iter()
            .copied()
            .zip(callable.parameters.iter().copied())
            .map(|(parameter, semantic)| -> Result<ParameterSlot, SyntaxModuleEmissionError> {
                let unavailable = |fact: &str| {
                    emission_verification(format!(
                        "lambda parameter {fact} is unavailable at {} (lambda {})",
                        beskid_queries::format_ast_node_site(db, parameter),
                        beskid_queries::format_ast_node_site(db, lambda),
                    ))
                };
                let slot = local_slot(db, parameter).ok().flatten().ok_or_else(|| unavailable("slot"))?;
                let managed_reference = match managed_reference_kind(db, parameter)
                    .ok()
                    .flatten()
                    .ok_or_else(|| unavailable("managed reference kind"))?
                {
                    ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                    ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                };
                Ok(ParameterSlot {
                    slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                    value_type: map_signature_type(isa, semantic).ok_or_else(|| unavailable("ABI type"))?,
                    managed_reference,
                })
            })
            .collect::<Result<Vec<_>, SyntaxModuleEmissionError>>()?;
        // Collect closure captures if present.
        let closure_captures = {
            if environment.captures.is_empty() {
                None
            } else {
                let authority = if is_event_handler {
                    input.event_handler_closure_lowering_authority(lambda, lambda)
                } else {
                    input.closure_lowering_authority(lambda, lambda)
                };
                let Some(authority) = authority else {
                    continue;
                };
                let Some(captures) = authority
                    .plan
                    .captures
                    .iter()
                    .map(|field| {
                        Some(beskid_isle::InlineCaptureField {
                            local_slot: beskid_isle::LocalSlotId {
                                owner_node: field.capture.slot.owner.node.0,
                                index: field.capture.slot.index,
                            },
                            field_offset: u32::try_from(field.field_offset).ok()?,
                            pointer_map_index: field.pointer_map_index,
                            value_type: map_signature_type(isa, field.abi_type)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                Some(captures)
            }
        };
        if closure_captures.is_some() {
            signature.params.insert(0, AbiParam::new(isa.pointer_type()));
        }
        let symbol = format!("__beskid_lambda_entry_syntax_g{}_n{}", input.generation_tag(lambda.generation), lambda.node.0);
        trampolines.push(LambdaTrampoline {
            lambda,
            callee: DirectCallee::lambda_trampoline(lambda),
            specialization: None,
            lambda_body,
            target_signature: signature,
            parameters,
            closure_captures,
            symbol,
        });
    }
    Ok(trampolines)
}

/// One lambda of a generic body in the enclosing instance `instance`: its callable ABI, parameter
/// classes and capture layout come from that instance, and its entry symbol extends the owner's.
fn resolve_specialized_lambda_trampoline(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    lambda: AstNodeKey,
    instance: &GenericSpecializationInstance,
    owner_symbol: &str,
) -> Result<LambdaTrampoline, SyntaxModuleEmissionError> {
    let db = input.database();
    let site = || format!("{} in `{owner_symbol}`", beskid_queries::format_ast_node_site(db, lambda));
    let fact = specialized_closure_signature(db, lambda, instance)
        .map_err(|error| emission_verification(format!("lambda {} has no specialized signature: {error}", site())))?
        .ok_or_else(|| emission_verification(format!("lambda {} has no specialized signature", site())))?;
    let mut signature = signature_for_item(isa, fact.callable.clone())
        .ok_or_else(|| emission_verification(format!("lambda {} has no target signature", site())))?;
    let environment = closure_environment(db, lambda)
        .map_err(|error| emission_verification(error.to_string()))?
        .ok_or_else(|| emission_verification(format!("lambda {} has no closure environment", site())))?;
    if environment.parameters.len() != fact.callable.parameters.len()
        || fact.parameter_managed.len() != fact.callable.parameters.len()
    {
        return Err(emission_verification(format!("lambda {} parameter facts do not match its signature", site())));
    }
    let parameters = environment
        .parameters
        .iter()
        .zip(fact.callable.parameters.iter().zip(fact.parameter_managed.iter()))
        .map(|(parameter, (semantic, managed))| -> Result<ParameterSlot, SyntaxModuleEmissionError> {
            let slot = local_slot(db, *parameter)
                .ok()
                .flatten()
                .ok_or_else(|| emission_verification(format!("lambda {} parameter slot is unavailable", site())))?;
            Ok(ParameterSlot {
                slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                value_type: map_signature_type(isa, *semantic)
                    .ok_or_else(|| emission_verification(format!("lambda {} parameter ABI is unavailable", site())))?,
                managed_reference: match managed {
                    ManagedReferenceKind::GcManaged => ManagedReferenceFact::GcManaged,
                    ManagedReferenceKind::NativeOrScalar => ManagedReferenceFact::NativeOrScalar,
                },
            })
        })
        .collect::<Result<Vec<_>, SyntaxModuleEmissionError>>()?;
    let closure_captures = if fact.fields.is_empty() {
        None
    } else {
        let authority = input.specialized_closure_lowering_authority(lambda, lambda, instance).ok_or_else(|| {
            emission_verification(format!("lambda {} has no closure allocation authority", site()))
        })?;
        Some(
            authority
                .plan
                .captures
                .iter()
                .map(|field| {
                    Some(beskid_isle::InlineCaptureField {
                        local_slot: beskid_isle::LocalSlotId {
                            owner_node: field.capture.slot.owner.node.0,
                            index: field.capture.slot.index,
                        },
                        field_offset: u32::try_from(field.field_offset).ok()?,
                        pointer_map_index: field.pointer_map_index,
                        value_type: map_signature_type(isa, field.abi_type)?,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| emission_verification(format!("lambda {} capture layout is unavailable", site())))?,
        )
    };
    if closure_captures.is_some() {
        signature.params.insert(0, AbiParam::new(isa.pointer_type()));
    }
    Ok(LambdaTrampoline {
        lambda,
        callee: DirectCallee::specialized_lambda_trampoline(lambda, generic_specialization_identity(instance)),
        specialization: Some(instance.clone()),
        lambda_body: fact.body,
        target_signature: signature,
        parameters,
        closure_captures,
        symbol: format!("{owner_symbol}#lambda_g{}_n{}", input.generation_tag(lambda.generation), lambda.node.0),
    })
}

fn collect_lambda_nodes(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    lambdas: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::LambdaExpression) {
        lambdas.push(key);
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_lambda_nodes(db, child, visited, lambdas);
        }
    }
}
