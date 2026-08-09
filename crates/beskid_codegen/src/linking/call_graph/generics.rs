use std::path::PathBuf;

use beskid_analysis::hir::{HirCallExpression, HirExpressionNode};
use beskid_analysis::resolve::{ItemId, Resolution};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};

use crate::lowering::types::type_id_for_type;

pub(super) fn generic_type_args_for_call(
    call: &Spanned<HirCallExpression>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Vec<TypeId> {
    let HirExpressionNode::PathExpression(path_expr) = &call.node.callee.node else {
        return Vec::new();
    };
    let Some(segment) = path_expr.node.path.node.segments.last() else {
        return Vec::new();
    };
    segment
        .node
        .type_args
        .iter()
        .filter_map(|arg| type_id_for_type(resolution, type_result, source_path, arg))
        .collect()
}
pub(super) fn infer_generic_type_args_for_call(
    call: &Spanned<HirCallExpression>,
    item_id: ItemId,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<Vec<TypeId>> {
    let expected = type_result.generic_items.get(&item_id)?.len();
    if expected == 0 {
        return Some(Vec::new());
    }

    if let Some(expr_type) = type_result.node_type(call.id)
        && let Some(args) = infer_generic_args_from_call_expr_type(type_result, item_id, expr_type)
    {
        return Some(args);
    }

    let mut arg_types = Vec::with_capacity(call.node.args.len());
    for arg in &call.node.args {
        arg_types.push(expr_type_for_call_arg(arg, resolution, type_result, source_path)?);
    }
    type_result.infer_generic_args_from_call_types(item_id, &arg_types)
}

fn infer_generic_args_from_call_expr_type(
    type_result: &TypeResult,
    item_id: ItemId,
    expr_type: TypeId,
) -> Option<Vec<TypeId>> {
    let generic_names = type_result.generic_items.get(&item_id)?;
    let expected = generic_names.len();
    if expected == 0 {
        return Some(Vec::new());
    }
    if let Some(TypeInfo::Applied { args, .. }) = type_result.types.get(expr_type)
        && args.len() == expected
    {
        return Some(args.clone());
    }
    let signature = type_result.function_signatures.get(&item_id)?;
    let mut mapping = std::collections::HashMap::new();
    if !bind_generic_args_from_return_type(&type_result.types, signature.return_type, expr_type, &mut mapping)
        || mapping.len() != expected
    {
        return None;
    }
    let mut substitution = Vec::with_capacity(expected);
    for name in generic_names {
        substitution.push(*mapping.get(name)?);
    }
    Some(substitution)
}

fn bind_generic_args_from_return_type(
    types: &beskid_analysis::types::TypeTable,
    param_type: TypeId,
    arg_type: TypeId,
    mapping: &mut std::collections::HashMap<String, TypeId>,
) -> bool {
    match types.get(param_type) {
        Some(TypeInfo::GenericParam(name)) => {
            if let Some(existing) = mapping.get(name) {
                *existing == arg_type
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
                if !bind_generic_args_from_return_type(types, *param, *arg, mapping) {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

fn expr_type_for_call_arg(
    arg: &Spanned<HirExpressionNode>,
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    if let Some(type_id) = type_result.node_type(arg.id) {
        return Some(type_id);
    }
    if let HirExpressionNode::PathExpression(path) = &arg.node {
        let span = path.node.path.span;
        if let Some(local_id) = resolution.tables.local_id_for_span(span, source_path)
            && let Some(type_id) = type_result.local_types.get(&local_id)
        {
            return Some(*type_id);
        }
    }
    None
}
