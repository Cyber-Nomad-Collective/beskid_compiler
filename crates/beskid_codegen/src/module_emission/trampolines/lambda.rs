use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee};
use beskid_queries::{child_nodes, closure_environment, closure_signature, node_kind};
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
        let Some(mut signature) = signature_for_item(isa, lambda_sig.callable) else {
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
