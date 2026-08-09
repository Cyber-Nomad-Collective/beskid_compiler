use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee};
use beskid_queries::{
    ItemSignature, SemanticTypeId, child_nodes, closure_environment, closure_signature, item_abi_signature, node_kind,
    resolved_item, spawn_entry_validation,
};
use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Function, InstBuilder, Signature, Type, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::items::{ResolvedSyntaxModuleItem, syntax_item_symbol};
use crate::CodegenInput;

#[derive(Debug, Clone)]
pub(super) struct SpawnTrampoline {
    pub(super) spawn: AstNodeKey,
    pub(super) target_symbol: String,
    pub(super) target_signature: Signature,
    pub(super) lambda_body: Option<AstNodeKey>,
    /// Present when the trampoline target is a capturing lambda that reads from the environment.
    pub(super) closure_captures: Option<Vec<beskid_isle::InlineCaptureField>>,
    pub(super) symbol: String,
}

/// One freestanding lambda lowered to its own trampoline function.
#[derive(Debug, Clone)]
pub(super) struct LambdaTrampoline {
    pub(super) lambda: AstNodeKey,
    pub(super) lambda_body: AstNodeKey,
    pub(super) target_signature: Signature,
    pub(super) closure_captures: Option<Vec<beskid_isle::InlineCaptureField>>,
    pub(super) symbol: String,
}

/// Spawn has no ordinary CallExpression edge, so the generic direct-call reachability query does
/// not include its target. Add only entries proven by the same strict direct-item validation used
/// for trampoline generation; this does not make lambda or argument-bearing spawns reachable.
pub(super) fn expand_direct_spawn_items(
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
            if !validation.is_zero_argument_entry
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

/// Resolve source-proven zero-argument entries without ever re-entering HIR lowering.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
pub(super) fn resolve_spawn_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
) -> Result<Vec<SpawnTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut spawns = Vec::new();
    let mut visited = HashSet::new();
    for item in items {
        collect_spawn_nodes(db, item.key, &mut visited, &mut spawns);
    }
    let mut trampolines = Vec::new();
    for spawn in spawns {
        let Some(validation) =
            spawn_entry_validation(db, spawn).map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        if !validation.is_zero_argument_entry {
            continue;
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
                    .and_then(|signature| spawn_target_signature(isa, signature))
                else {
                    continue;
                };
                if !signature.params.is_empty() {
                    continue;
                }
                let callee = DirectCallee::item(target.declaration);
                let Some(target_symbol) = symbols.get(&callee).cloned() else {
                    continue;
                };
                let symbol = spawn_trampoline_symbol(&target_symbol, spawn);
                trampolines.push(SpawnTrampoline {
                    spawn,
                    target_symbol,
                    target_signature: signature,
                    lambda_body: None,
                    closure_captures: None,
                    symbol,
                });
            }
            Some(beskid_queries::IndexedNodeKind::LambdaExpression) => {
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
                                value_type: map_spawn_capture_type(isa, field.abi_type)?,
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
                let Some(mut signature) = spawn_target_signature(isa, lambda.callable) else {
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
                trampolines.push(SpawnTrampoline {
                    spawn,
                    target_symbol,
                    target_signature: signature,
                    lambda_body: Some(lambda.body),
                    closure_captures,
                    symbol,
                });
            }
            _ => continue,
        }
    }
    Ok(trampolines)
}

fn spawn_trampoline_symbol(target_symbol: &str, spawn: AstNodeKey) -> String {
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

/// Resolve trampoline entries for every freestanding [`LambdaExpression`] in the syntax tree.
///
/// Capture-free lambdas emit a simple entry function. Capturing lambdas require
/// generation-safe allocate/store/root authority before the entry is emitted.
pub(super) fn resolve_lambda_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    _symbols: &HashMap<DirectCallee, String>,
) -> Result<Vec<LambdaTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut lambdas = Vec::new();
    let mut visited = HashSet::new();
    for item in items {
        collect_lambda_nodes(db, item.key, &mut visited, &mut lambdas);
    }
    let mut trampolines = Vec::new();
    for lambda in lambdas {
        let Some(lambda_sig) =
            closure_signature(db, lambda).map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        let Some(mut signature) = spawn_target_signature(isa, lambda_sig.callable) else {
            continue;
        };
        // Collect closure captures if present.
        let closure_captures = {
            let Some(environment) =
                closure_environment(db, lambda).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            if environment.captures.is_empty() {
                None
            } else {
                let Some(authority) = input.closure_lowering_authority(lambda, lambda) else {
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
                            value_type: map_spawn_capture_type(isa, field.abi_type)?,
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
        let symbol = format!("__beskid_lambda_entry_syntax_g{}_n{}", lambda.generation.0, lambda.node.0);
        trampolines.push(LambdaTrampoline {
            lambda,
            lambda_body: lambda_sig.body,
            target_signature: signature,
            closure_captures,
            symbol,
        });
    }
    Ok(trampolines)
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

fn collect_spawn_nodes(
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
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_spawn_nodes(db, child, visited, spawns);
        }
    }
}

pub(super) fn emit_spawn_trampoline(
    trampoline: &SpawnTrampoline,
    isa: &dyn TargetIsa,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let mut signature = Signature::new(isa.default_call_conv());
    signature.params.push(AbiParam::new(pointer));
    signature.returns.push(AbiParam::new(types::I64));
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let environment = builder.block_params(entry)[0];
        let target_signature = builder.import_signature(trampoline.target_signature.clone());
        let target = builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(trampoline.target_symbol.as_bytes()),
            signature: target_signature,
            colocated: false,
            patchable: false,
        });
        let call = if trampoline.closure_captures.is_some() {
            builder.ins().call(target, &[environment])
        } else {
            builder.ins().call(target, &[])
        };
        let results = builder.inst_results(call).to_vec();
        let result = match results.as_slice() {
            [] => builder.ins().iconst(types::I64, 0),
            [value] if builder.func.dfg.value_type(*value) == types::I64 => *value,
            [value] if builder.func.dfg.value_type(*value).is_int() => builder.ins().sextend(types::I64, *value),
            _ => {
                return Err(emission_verification(format!(
                    "spawn trampoline target `{}` must return unit or an integer ABI value",
                    trampoline.target_symbol
                )));
            }
        };
        builder.ins().return_(&[result]);
        builder.finalize();
    }
    verify_function(&function, isa.flags()).map_err(|error| {
        emission_verification(format!("spawn trampoline `{}` verification failed: {error}", trampoline.symbol))
    })?;
    Ok(function)
}

fn map_spawn_capture_type(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
    match semantic {
        SemanticTypeId::BOOL | SemanticTypeId::U8 => Some(types::I8),
        SemanticTypeId::I32 | SemanticTypeId::CHAR => Some(types::I32),
        SemanticTypeId::I64 => Some(types::I64),
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING => Some(isa.pointer_type()),
        SemanticTypeId::F64 => Some(types::F64),
        _ => None,
    }
}

fn spawn_target_signature(isa: &dyn TargetIsa, item: ItemSignature) -> Option<Signature> {
    fn map(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
        Some(match semantic {
            SemanticTypeId::BOOL | SemanticTypeId::U8 => types::I8,
            SemanticTypeId::I32 => types::I32,
            SemanticTypeId::I64 => types::I64,
            SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING => isa.pointer_type(),
            SemanticTypeId::F64 => types::F64,
            SemanticTypeId::CHAR => types::I32,
            SemanticTypeId::UNIT | SemanticTypeId::NEVER => return None,
            _ => return None,
        })
    }

    let mut signature = Signature::new(isa.default_call_conv());
    signature.params.extend(
        item.parameters
            .iter()
            .copied()
            .map(|semantic| map(isa, semantic).map(AbiParam::new))
            .collect::<Option<Vec<_>>>()?,
    );
    if !matches!(item.result, SemanticTypeId::UNIT | SemanticTypeId::NEVER) {
        signature.returns.push(AbiParam::new(map(isa, item.result)?));
    }
    Some(signature)
}
