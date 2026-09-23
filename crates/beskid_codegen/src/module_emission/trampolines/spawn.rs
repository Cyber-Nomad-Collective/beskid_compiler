use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee};
use beskid_queries::{closure_environment, closure_signature, item_abi_signature, node_kind, resolved_item, spawn_entry_validation};
use cranelift_codegen::ir::AbiParam;
use cranelift_codegen::isa::TargetIsa;

use super::types::SpawnTrampoline;
use crate::CodegenInput;
use crate::isle_adapter::mappings::{map_signature_type, signature_for_item};
use crate::module_emission::contracts::{SyntaxModuleEmissionError, emission_verification};
use crate::module_emission::items::{ResolvedSyntaxModuleItem, syntax_item_symbol};

/// Spawn has no ordinary CallExpression edge, so the generic direct-call reachability query does
/// not include its target. Add only entries proven by the same strict direct-item validation used
/// for trampoline generation, with or without eager arguments; lambda entries are emitted as
/// their own trampoline bodies.
pub(in crate::module_emission) fn expand_direct_spawn_items(
    input: &CodegenInput<'_>,
    mut items: Vec<ResolvedSyntaxModuleItem>,
) -> Result<Vec<ResolvedSyntaxModuleItem>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut cursor = 0;
    while cursor < items.len() {
        let mut spawns = Vec::new();
        collect_spawn_nodes(db, items[cursor].key, &mut HashSet::new(), &mut spawns);
        for spawn in spawns {
            let Some(validation) =
                spawn_entry_validation(db, spawn).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            if !validation.is_legal_entry
                || node_kind(db, validation.target).map_err(|error| emission_verification(error.to_string()))?
                    != Some(beskid_queries::IndexedNodeKind::PathExpression)
            {
                continue;
            }
            let Some(target) =
                resolved_item(db, validation.target).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            if items.iter().any(|item| item.key == target.declaration) {
                continue;
            }
            let Some(symbol) = syntax_item_symbol(input, target.declaration) else {
                continue;
            };
            if item_abi_signature(db, target.declaration)
                .map_err(|error| emission_verification(error.to_string()))?
                .is_none()
            {
                continue;
            }
            items.push(ResolvedSyntaxModuleItem {
                key: target.declaration,
                symbol,
                callee: DirectCallee::item(target.declaration),
                specialization: None,
            });
        }
        cursor += 1;
    }
    Ok(items)
}

/// Resolve source-proven fiber entries from generation-safe facts.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Direct
/// items with eager arguments also require an artifact-owned argument environment plan; capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
pub(in crate::module_emission) fn resolve_spawn_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
) -> Result<Vec<SpawnTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut spawns = Vec::new();
    let mut visited = HashSet::new();
    for item in items {
        if let Some(ownership) = beskid_queries::callable_fiber_ownership(db, item.key)
            .map_err(|error| emission_verification(error.to_string()))?
            && !ownership.diagnostics.is_empty()
        {
            return Err(emission_verification(format!(
                "spawn legality rejected {}: {:?}",
                beskid_queries::format_ast_node_key(db, item.key),
                ownership.diagnostics
            )));
        }
        collect_spawn_nodes(db, item.key, &mut visited, &mut spawns);
    }
    let mut trampolines = Vec::new();
    for spawn in spawns {
        let Some(validation) =
            spawn_entry_validation(db, spawn).map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        if !validation.is_legal_entry {
            return Err(emission_verification(format!(
                "spawn legality rejected {} at SpawnExpression: {:?}",
                spawn.unit.path(db).display(),
                validation.diagnostics,
            )));
        }
        match node_kind(db, validation.target).map_err(|error| emission_verification(error.to_string()))? {
            Some(beskid_queries::IndexedNodeKind::PathExpression) => {
                let Some(target) =
                    resolved_item(db, validation.target).map_err(|error| emission_verification(error.to_string()))?
                else {
                    continue;
                };
                let Some(signature) = item_abi_signature(db, target.declaration)
                    .map_err(|error| emission_verification(error.to_string()))?
                    .and_then(|signature| signature_for_item(isa, signature))
                else {
                    continue;
                };
                let argument_plan = if validation.arguments.is_empty() {
                    None
                } else {
                    Some(input.spawn_argument_static_plan(spawn).ok_or_else(|| {
                        emission_verification(format!(
                            "spawn argument environment unavailable for {}",
                            beskid_queries::format_ast_node_key(db, spawn)
                        ))
                    })?)
                };
                if signature.params.len() != argument_plan.as_ref().map_or(0, |plan| plan.fields.len()) {
                    continue;
                }
                let callee = DirectCallee::item(target.declaration);
                let Some(target_symbol) = symbols.get(&callee).cloned() else {
                    continue;
                };
                let symbol = spawn_trampoline_symbol(&target_symbol, spawn);
                let result_plan =
                    spawn_result_plan(input, spawn, &symbol, validation.callable.as_ref().unwrap().result)?;
                trampolines.push(SpawnTrampoline {
                    spawn,
                    target_symbol,
                    target_signature: signature,
                    lambda_body: None,
                    closure_captures: None,
                    argument_plan,
                    symbol,
                    result_plan,
                });
            }
            Some(beskid_queries::IndexedNodeKind::LambdaExpression) => {
                if !validation.arguments.is_empty() {
                    continue;
                }
                let Some(environment) = closure_environment(db, validation.target)
                    .map_err(|error| emission_verification(error.to_string()))?
                else {
                    continue;
                };
                let closure_captures = if environment.captures.is_empty() {
                    None
                } else {
                    let Some(authority) = input.closure_lowering_authority(spawn, validation.target) else {
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
                };
                let Some(lambda) = closure_signature(db, validation.target)
                    .map_err(|error| emission_verification(error.to_string()))?
                else {
                    continue;
                };
                let Some(mut signature) = signature_for_item(isa, lambda.callable.clone()) else {
                    continue;
                };
                if !signature.params.is_empty() {
                    continue;
                }
                if closure_captures.is_some() {
                    signature.params.insert(0, AbiParam::new(isa.pointer_type()));
                }
                let target_symbol = format!("__beskid_spawn_lambda_syntax_g{}_n{}", spawn.generation.0, spawn.node.0);
                let symbol = spawn_trampoline_symbol(&target_symbol, spawn);
                let result_plan = spawn_result_plan(input, spawn, &symbol, lambda.callable.result)?;
                trampolines.push(SpawnTrampoline {
                    spawn,
                    target_symbol,
                    target_signature: signature,
                    lambda_body: Some(lambda.body),
                    closure_captures,
                    argument_plan: None,
                    symbol,
                    result_plan,
                });
            }
            _ => continue,
        }
    }
    Ok(trampolines)
}

pub(in crate::module_emission) fn spawn_result_plan(
    input: &CodegenInput<'_>,
    spawn: AstNodeKey,
    symbol: &str,
    result: beskid_queries::SemanticTypeId,
) -> Result<crate::aggregate_static::AggregateStaticPlan, SyntaxModuleEmissionError> {
    let header = input
        .abi_manifest()
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidObjectHeader")
        .ok_or_else(|| emission_verification("spawn result object header unavailable"))?;
    let handle = beskid_queries::spawn_handle_type(input.database(), spawn)
        .map_err(|error| emission_verification(error.to_string()))?
        .ok_or_else(|| emission_verification("spawn payload source identity unavailable"))?;
    let pointer_map_offsets =
        if handle.payload.managed_reference_kind() == beskid_queries::ManagedReferenceKind::GcManaged {
            vec![header.size]
        } else {
            Vec::new()
        };
    Ok(crate::aggregate_static::AggregateStaticPlan {
        literal: spawn,
        descriptor_symbol: format!("{symbol}_result_descriptor"),
        pointer_map_symbol: format!("{symbol}_result_pointer_map"),
        allocation_request_symbol: format!("{symbol}_result_request"),
        object_size: header.size + 8,
        object_alignment: header.alignment,
        pointer_map_offsets: pointer_map_offsets.into(),
        fields: vec![crate::aggregate_static::AggregateStaticField { abi_type: result, field_offset: header.size }]
            .into(),
    })
}

pub(in crate::module_emission) fn spawn_trampoline_symbol(target_symbol: &str, spawn: AstNodeKey) -> String {
    format!(
        "__beskid_spawn_entry_syntax_{}_g{}_n{}",
        target_symbol
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
            .collect::<String>(),
        spawn.generation.0,
        spawn.node.0,
    )
}

pub(super) fn collect_spawn_nodes(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    spawns: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::SpawnExpression) {
        spawns.push(key);
    }
    if let Ok(Some(children)) = beskid_queries::child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_spawn_nodes(db, child, visited, spawns);
        }
    }
}
