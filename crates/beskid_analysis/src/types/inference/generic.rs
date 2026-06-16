//! Generic argument inference from call-site value types.

use std::collections::HashMap;

use crate::resolve::ItemId;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::signature::FunctionSignature;
use super::unify::unify_numeric_types;

pub fn infer_generic_args_from_call_types(
    types: &TypeTable,
    generic_items: &HashMap<ItemId, Vec<String>>,
    function_signatures: &HashMap<ItemId, FunctionSignature>,
    item_id: ItemId,
    arg_types: &[TypeId],
) -> Option<Vec<TypeId>> {
    let generic_names = generic_items.get(&item_id)?.clone();
    let expected_len = generic_names.len();
    if expected_len == 0 {
        return Some(Vec::new());
    }

    let params = function_signatures.get(&item_id)?.params.clone();
    let mut mapping: HashMap<String, TypeId> = HashMap::new();
    for (arg_type, param_type) in arg_types.iter().zip(params.iter()) {
        if !bind_generic_inference(types, *param_type, *arg_type, &mut mapping) {
            return None;
        }
    }
    if mapping.len() != expected_len {
        return None;
    }
    let mut substitution = Vec::with_capacity(expected_len);
    for name in generic_names {
        substitution.push(*mapping.get(&name)?);
    }
    Some(substitution)
}

fn bind_generic_inference(
    types: &TypeTable,
    param_type: TypeId,
    arg_type: TypeId,
    mapping: &mut HashMap<String, TypeId>,
) -> bool {
    match types.get(param_type) {
        Some(TypeInfo::GenericParam(name)) => {
            if let Some(existing) = mapping.get(name) {
                if *existing == arg_type {
                    true
                } else if let Some(unified) = unify_numeric_types(types, *existing, arg_type) {
                    mapping.insert(name.clone(), unified);
                    true
                } else {
                    false
                }
            } else {
                mapping.insert(name.clone(), arg_type);
                true
            }
        }
        Some(TypeInfo::Applied {
            base: param_base,
            args: param_args,
        }) => {
            let Some(TypeInfo::Applied {
                base: arg_base,
                args: arg_args,
            }) = types.get(arg_type)
            else {
                return false;
            };
            if param_base != arg_base || param_args.len() != arg_args.len() {
                return false;
            }
            for (param, arg) in param_args.iter().zip(arg_args.iter()) {
                if !bind_generic_inference(types, *param, *arg, mapping) {
                    return false;
                }
            }
            true
        }
        Some(TypeInfo::Array(param_elem)) => {
            if let Some(TypeInfo::Array(arg_elem)) = types.get(arg_type) {
                bind_generic_inference(types, *param_elem, *arg_elem, mapping)
            } else {
                false
            }
        }
        _ => true,
    }
}
