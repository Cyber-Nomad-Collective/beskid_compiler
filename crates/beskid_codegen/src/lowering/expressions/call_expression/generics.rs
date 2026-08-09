use std::collections::HashMap;

use super::{HirExpressionNode, NodeLoweringContext, Spanned, TypeId, resolve_path_base_local};

pub(super) fn infer_generic_args_from_call(
    type_result: &beskid_analysis::types::TypeResult,
    item_id: beskid_analysis::resolve::ItemId,
    args: &[Spanned<HirExpressionNode>],
    ctx: &NodeLoweringContext<'_, '_>,
) -> Option<Vec<TypeId>> {
    let mut arg_types = Vec::with_capacity(args.len());
    for arg in args {
        if let HirExpressionNode::PathExpression(path) = &arg.node {
            let segments = &path.node.path.node.segments;
            if segments.len() == 1 {
                let name = segments[0].node.name.node.name.as_str();
                if let Some(local_id) = resolve_path_base_local(
                    ctx.resolution,
                    path.node.path.span,
                    name,
                    ctx.codegen.current_source_path.as_ref(),
                ) && let Some(type_id) = ctx
                    .state
                    .local_type_overrides
                    .get(&local_id)
                    .copied()
                    .or_else(|| ctx.type_result.local_types.get(&local_id).copied())
                {
                    arg_types.push(type_id);
                    continue;
                }
            }
        }
        let type_id =
            ctx.require_expr_type_for_node(arg).ok().or_else(|| ctx.expr_type_for_node(arg)).or_else(|| {
                if let HirExpressionNode::PathExpression(path) = &arg.node {
                    crate::lowering::locals::local_id_for_span(
                        ctx.resolution,
                        path.node.path.span,
                        ctx.codegen.current_source_path.as_ref(),
                    )
                    .and_then(|local_id| {
                        ctx.state
                            .local_type_overrides
                            .get(&local_id)
                            .copied()
                            .or_else(|| ctx.type_result.local_types.get(&local_id).copied())
                    })
                } else {
                    None
                }
            })?;
        arg_types.push(type_id);
    }
    type_result.infer_generic_args_from_call_types(item_id, &arg_types)
}

pub(super) fn infer_generic_args_from_call_expr_type(
    type_result: &beskid_analysis::types::TypeResult,
    item_id: beskid_analysis::resolve::ItemId,
    expr_type: Option<TypeId>,
) -> Option<Vec<TypeId>> {
    let expr_type = expr_type?;
    let generic_names = type_result.generic_items.get(&item_id)?;
    let expected = generic_names.len();
    if expected == 0 {
        return Some(Vec::new());
    }
    if let Some(beskid_analysis::types::TypeInfo::Applied { args, .. }) = type_result.types.get(expr_type)
        && args.len() == expected
    {
        return Some(args.clone());
    }
    let signature = type_result.function_signatures.get(&item_id)?;
    let mut mapping = HashMap::new();
    if !bind_generic_args_from_types(&type_result.types, signature.return_type, expr_type, &mut mapping)
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

fn bind_generic_args_from_types(
    types: &beskid_analysis::types::TypeTable,
    param_type: TypeId,
    arg_type: TypeId,
    mapping: &mut HashMap<String, TypeId>,
) -> bool {
    match types.get(param_type) {
        Some(beskid_analysis::types::TypeInfo::GenericParam(name)) => {
            if let Some(existing) = mapping.get(name) {
                *existing == arg_type
            } else {
                mapping.insert(name.clone(), arg_type);
                true
            }
        }
        Some(beskid_analysis::types::TypeInfo::Applied { base: param_base, args: param_args }) => {
            let Some(beskid_analysis::types::TypeInfo::Applied { base: arg_base, args: arg_args }) =
                types.get(arg_type)
            else {
                return false;
            };
            if param_base != arg_base || param_args.len() != arg_args.len() {
                return false;
            }
            for (param, arg) in param_args.iter().zip(arg_args.iter()) {
                if !bind_generic_args_from_types(types, *param, *arg, mapping) {
                    return false;
                }
            }
            true
        }
        Some(beskid_analysis::types::TypeInfo::Array(param_elem)) => {
            if let Some(beskid_analysis::types::TypeInfo::Array(arg_elem)) = types.get(arg_type) {
                bind_generic_args_from_types(types, *param_elem, *arg_elem, mapping)
            } else {
                false
            }
        }
        _ => true,
    }
}
