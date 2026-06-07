//! Shared generic argument inference from call-site value types.

use std::collections::HashMap;

use crate::hir::HirPrimitiveType;
use crate::resolve::ItemId;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::context::context::{FunctionSignature, TypeResult};

/// Infer generic type arguments for a call from already-known argument types.
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

impl TypeResult {
    /// Infer generic type arguments for a call from already-known argument types.
    pub fn infer_generic_args_from_call_types(
        &self,
        item_id: ItemId,
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        infer_generic_args_from_call_types(
            &self.types,
            &self.generic_items,
            &self.function_signatures,
            item_id,
            arg_types,
        )
    }
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
                } else if let Some(unified) = unify_generic_binding_types(types, *existing, arg_type)
                {
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

fn unify_generic_binding_types(
    types: &TypeTable,
    left: TypeId,
    right: TypeId,
) -> Option<TypeId> {
    if left == right {
        return Some(left);
    }
    if is_numeric(types, left) && is_numeric(types, right) {
        let i64_id = types.find_primitive(HirPrimitiveType::I64)?;
        let left_is_i64 = matches!(
            types.get(left),
            Some(TypeInfo::Primitive(HirPrimitiveType::I64))
        );
        let right_is_i64 = matches!(
            types.get(right),
            Some(TypeInfo::Primitive(HirPrimitiveType::I64))
        );
        if left_is_i64 || right_is_i64 {
            return Some(i64_id);
        }
        return Some(left);
    }
    None
}

fn is_numeric(types: &TypeTable, type_id: TypeId) -> bool {
    matches!(
        types.get(type_id),
        Some(TypeInfo::Primitive(
            HirPrimitiveType::I32
                | HirPrimitiveType::I64
                | HirPrimitiveType::U8
                | HirPrimitiveType::F64
        ))
    )
}
