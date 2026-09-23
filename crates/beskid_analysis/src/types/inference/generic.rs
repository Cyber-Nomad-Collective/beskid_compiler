//! Generic argument inference from call-site value types.

use std::collections::HashMap;

use crate::resolve::ItemId;
use crate::syntax::{Expression, Literal, Spanned, UnaryOp};
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::unify::unify_numeric_types;
use crate::types::result::FunctionSignature;

/// One generic parameter bound to two call arguments with different declared primitive types.
/// `first`/`second` name the conflicting types in argument-position order; `second_arg_index`
/// is the position (into the call's argument list) of the second, conflicting occurrence.
pub struct GenericParameterConflict {
    pub parameter: String,
    pub first: TypeId,
    pub second: TypeId,
    pub second_arg_index: usize,
}

/// Whether `expression` is an integer literal with no explicit type suffix (`2`, not `2_i64`).
/// A bare literal's default type (`i32`) is provisional: it adapts to whatever concrete type its
/// generic parameter resolves to from another, explicitly-typed argument, so it must never be
/// treated as one of the two "independently declared" sides of a real conflict.
fn is_bare_integer_literal(expression: &Expression) -> bool {
    // A leading `-` (e.g. `-1`) still names a bare literal magnitude; only its sign changes.
    let expression = match expression {
        Expression::Unary(unary) if unary.node.op.node == UnaryOp::Neg => &unary.node.expr.node,
        other => other,
    };
    let Expression::Literal(literal) = expression else {
        return false;
    };
    let Literal::Integer(text) = &literal.node.literal.node else {
        return false;
    };
    !(text.ends_with("_u8") || text.ends_with("_i64") || text.ends_with("_i32") || text.ends_with("_u32"))
}

/// Detect a generic parameter bound to two declared-distinct primitive types across a call's
/// direct arguments (e.g. `word` and `i64`) that `bind_generic_inference`'s numeric widening
/// would otherwise unify silently. `Assert.Equal<T>(T actual, T expected, ...)` requires an
/// exact type match between its `T`-typed arguments; numeric widening exists for bare integer
/// literals (which this function deliberately exempts, see [`is_bare_integer_literal`]) and
/// mixed-width binary operands, not for two independently-declared call results.
///
/// Only genuinely different, both-numeric primitive parameters are reported here — a
/// structurally incompatible pair (e.g. `bool` vs a nominal type) already fails
/// [`infer_generic_args_from_call_types`] outright with its own diagnostic, and a generic
/// parameter nested inside a composite argument type (e.g. `Box<T>`) is left to that same
/// existing path rather than risking a false positive here.
pub fn first_generic_parameter_conflict(
    types: &TypeTable,
    generic_items: &HashMap<ItemId, Vec<String>>,
    function_signatures: &HashMap<ItemId, FunctionSignature>,
    item_id: ItemId,
    arg_types: &[TypeId],
    args: &[Spanned<Expression>],
) -> Option<GenericParameterConflict> {
    if generic_items.get(&item_id).is_none_or(Vec::is_empty) {
        return None;
    }
    let params = function_signatures.get(&item_id)?.params.clone();
    // (bound type, still provisional because every occurrence bound so far was a bare literal)
    let mut bindings: HashMap<String, (TypeId, bool)> = HashMap::new();
    for (index, (arg_type, param_type)) in arg_types.iter().zip(params.iter()).enumerate() {
        let Some(TypeInfo::GenericParam(name)) = types.get(*param_type) else { continue };
        let this_is_literal = args.get(index).is_some_and(|arg| is_bare_integer_literal(&arg.node));
        let Some((existing_type, existing_is_literal)) = bindings.get(name).copied() else {
            bindings.insert(name.clone(), (*arg_type, this_is_literal));
            continue;
        };
        if existing_type == *arg_type {
            if !this_is_literal {
                bindings.insert(name.clone(), (*arg_type, false));
            }
            continue;
        }
        if unify_numeric_types(types, existing_type, *arg_type).is_none() {
            // Not even numeric-widening-compatible; `infer_generic_args_from_call_types` already
            // fails this call outright with its own diagnostic.
            continue;
        }
        if existing_is_literal || this_is_literal {
            // At least one side is a still-adapting bare literal: prefer the concrete
            // (non-literal) type as the binding, matching the widening this exempts.
            let (kept_type, kept_is_literal) =
                if existing_is_literal && !this_is_literal { (*arg_type, false) } else { (existing_type, existing_is_literal && this_is_literal) };
            bindings.insert(name.clone(), (kept_type, kept_is_literal));
            continue;
        }
        return Some(GenericParameterConflict {
            parameter: name.clone(),
            first: existing_type,
            second: *arg_type,
            second_arg_index: index,
        });
    }
    None
}

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
        Some(TypeInfo::Applied { base: param_base, args: param_args }) => {
            let Some(TypeInfo::Applied { base: arg_base, args: arg_args }) = types.get(arg_type) else {
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
