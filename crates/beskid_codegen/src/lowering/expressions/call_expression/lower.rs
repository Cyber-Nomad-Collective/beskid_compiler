use std::collections::HashMap;

use super::common::{lower_call_return, type_returns_runtime_value};
use super::contract_event::lower_contract_dispatch_call;
use super::event::lower_event_invoke_call;
use super::generics::{infer_generic_args_from_call, infer_generic_args_from_call_expr_type};
use super::indirect::{lower_indirect_function_call, lower_indirect_function_call_with_signature};
use super::lambda::{lower_function_typed_argument, lower_local_lambda_call};
use super::method::lower_method_dispatch_call;
use super::{
    AbiParam, BuiltinType, CallConv, CallLoweringKind, CodegenError, ExtFuncData, ExternalName, HirCallExpression,
    HirExpressionNode, HirLambdaExpression, InstBuilder, ItemKind, Lowerable, MemFlags, NodeLoweringContext,
    ResolvedValue, Signature, Spanned, TypeId, TypeInfo, Value, builtin_specs, call_kind_for_call, canonical_item_id,
    canonicalize_call_kind, dispatch_route_for_symbol, ensure_type_compatibility_or_expected, local_id_for_span,
    lower_dispatch_builtin_call, lower_function_with_name, lower_node, mangle_generic_item_function,
    mangle_item_function, mangle_method_name, map_type_id_to_clif, pointer_type, resolve_item_call_id,
    resolved_value_at, types,
};

impl Lowerable<NodeLoweringContext<'_, '_>> for HirCallExpression {
    type Output = Option<Value>;

    fn lower(node: &Spanned<Self>, ctx: &mut NodeLoweringContext<'_, '_>) -> Result<Self::Output, CodegenError> {
        let call_kind =
            call_kind_for_call(ctx.type_result, node).map(|kind| canonicalize_call_kind(ctx.resolution, kind));
        if let Some(CallLoweringKind::MethodDispatch { method_item_id, receiver_source, receiver_type }) = call_kind {
            return lower_method_dispatch_call(node, method_item_id, receiver_source, receiver_type, ctx);
        }
        if let Some(CallLoweringKind::EventInvoke { receiver_source, receiver_type }) = call_kind {
            return lower_event_invoke_call(node, receiver_source, receiver_type, ctx);
        }
        if let Some(CallLoweringKind::ContractDispatch { contract_item_id, receiver_source, .. }) = call_kind {
            return lower_contract_dispatch_call(node, contract_item_id, receiver_source, ctx);
        }

        fn lambda_from_callee<'a>(
            callee: &'a Spanned<HirExpressionNode>,
            ctx: &NodeLoweringContext<'_, '_>,
        ) -> Result<Option<&'a Spanned<HirLambdaExpression>>, CodegenError> {
            match &callee.node {
                HirExpressionNode::LambdaExpression(lambda) => Ok(Some(lambda)),
                HirExpressionNode::GroupedExpression(grouped) => lambda_from_callee(&grouped.node.expr, ctx),
                HirExpressionNode::PathExpression(path_expr) => {
                    let Some(resolved) = resolved_value_at(
                        ctx.resolution,
                        path_expr.node.path.span,
                        ctx.codegen.current_source_path.as_ref(),
                    ) else {
                        return Ok(None);
                    };
                    let ResolvedValue::Local(local_id) = resolved else {
                        return Ok(None);
                    };
                    let Some(lambda_ptr) = ctx.state.local_lambdas.get(&local_id).copied() else {
                        return Ok(None);
                    };
                    // SAFETY: pointer originates from an immutable borrow of HIR owned by the lowering context.
                    let lambda = unsafe { lambda_ptr.as_ref() }.ok_or(CodegenError::UnsupportedNode {
                        span: path_expr.node.path.span,
                        node: "dangling lambda binding",
                    })?;
                    Ok(Some(lambda))
                }
                _ => Ok(None),
            }
        }

        if let Some(lambda) = lambda_from_callee(&node.node.callee, ctx)? {
            return lower_local_lambda_call(node, lambda, ctx);
        }

        if let Some(callee_type_id) = ctx.expr_type(node.node.callee.id)
            && let Some(TypeInfo::Function { params, return_type }) = ctx.type_result.types.get(callee_type_id).cloned()
        {
            let callee_is_item_path = matches!(call_kind, Some(CallLoweringKind::ItemCall { .. }))
                || resolve_item_call_id(node, ctx.resolution, ctx.codegen.current_source_path.as_ref()).is_some()
                || if let HirExpressionNode::PathExpression(path_expr) = &node.node.callee.node {
                    matches!(
                        resolved_value_at(
                            ctx.resolution,
                            path_expr.node.path.span,
                            ctx.codegen.current_source_path.as_ref(),
                        ),
                        Some(ResolvedValue::Item(_))
                    )
                } else {
                    false
                };

            if !callee_is_item_path {
                let callee_value = lower_node(&node.node.callee, ctx)?.ok_or(CodegenError::UnsupportedNode {
                    span: node.node.callee.span,
                    node: "unit-valued function callee",
                })?;
                return lower_indirect_function_call_with_signature(node, callee_value, &params, return_type, ctx);
            }
        }

        let item_id = if let Some(CallLoweringKind::ItemCall { item_id }) = call_kind {
            item_id
        } else if let Some(item_id) =
            resolve_item_call_id(node, ctx.resolution, ctx.codegen.current_source_path.as_ref())
        {
            item_id
        } else if let HirExpressionNode::PathExpression(path_expr) = &node.node.callee.node {
            let resolved =
                resolved_value_at(ctx.resolution, path_expr.node.path.span, ctx.codegen.current_source_path.as_ref())
                    .ok_or(CodegenError::MissingResolvedValue { span: path_expr.node.path.span })?;

            match resolved {
                ResolvedValue::Item(item_id) => item_id,
                ResolvedValue::Local(local_id) => {
                    let local_type = ctx.type_result.local_types.get(&local_id).copied();
                    let local_is_function = local_type
                        .and_then(|type_id| ctx.type_result.types.get(type_id))
                        .is_some_and(|info| matches!(info, TypeInfo::Function { .. }));
                    if local_is_function {
                        return lower_indirect_function_call(node, local_id, ctx);
                    }

                    return Err(CodegenError::UnsupportedNode {
                        span: path_expr.node.path.span,
                        node: "non-item call target",
                    });
                }
            }
        } else {
            return Err(CodegenError::UnsupportedNode { span: node.node.callee.span, node: "non-path call callee" });
        };
        let item_id = canonical_item_id(ctx.resolution, item_id);

        let mut generic_args: Vec<TypeId> = Vec::new();
        if let HirExpressionNode::PathExpression(path_expr) = &node.node.callee.node {
            let segments = &path_expr.node.path.node.segments;
            for segment in
                [segments.last(), segments.len().checked_sub(2).and_then(|idx| segments.get(idx))].into_iter().flatten()
            {
                if segment.node.type_args.is_empty() {
                    continue;
                }
                let mut candidate = Vec::with_capacity(segment.node.type_args.len());
                for arg in &segment.node.type_args {
                    let Some(type_id) = crate::lowering::types::type_id_for_type(
                        ctx.resolution,
                        ctx.type_result,
                        ctx.codegen.current_source_path.as_ref(),
                        arg,
                    ) else {
                        candidate.clear();
                        break;
                    };
                    candidate.push(type_id);
                }
                if !candidate.is_empty() {
                    generic_args = candidate;
                    break;
                }
            }
        }

        let expected_generics = ctx.type_result.generic_items.get(&item_id).map(|names| names.len()).unwrap_or(0);

        if expected_generics != generic_args.len() {
            if generic_args.is_empty() && expected_generics > 0 {
                generic_args = infer_generic_args_from_call(ctx.type_result, item_id, &node.node.args, ctx)
                    .or_else(|| {
                        infer_generic_args_from_call_expr_type(ctx.type_result, item_id, ctx.expr_type(node.id))
                    })
                    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "generic argument mismatch" })?;
            } else {
                return Err(CodegenError::UnsupportedNode { span: node.span, node: "generic argument mismatch" });
            }
        }

        let signature = ctx
            .type_result
            .function_signatures
            .get(&item_id)
            .ok_or(CodegenError::MissingSymbol("function signature"))?;
        let builtin_param_kinds = ctx
            .resolution
            .builtin_items
            .get(&item_id)
            .and_then(|index| builtin_specs().get(*index))
            .map(|spec| spec.params.to_vec());

        let mut mapping = HashMap::new();
        if expected_generics > 0
            && let Some(names) = ctx.type_result.generic_items.get(&item_id)
        {
            for (name, arg) in names.iter().zip(generic_args.iter()) {
                mapping.insert(name.clone(), *arg);
            }
        }

        let substitute_type_id = |type_id: TypeId| -> TypeId {
            match ctx.type_result.types.get(type_id) {
                Some(TypeInfo::GenericParam(name)) => mapping.get(name).copied().unwrap_or(type_id),
                Some(TypeInfo::Applied { .. }) | Some(TypeInfo::Function { .. }) => type_id,
                _ => type_id,
            }
        };

        let substituted_params: Vec<TypeId> = signature.params.iter().map(|param| substitute_type_id(*param)).collect();
        let substituted_return = substitute_type_id(signature.return_type);

        let expected_arity = builtin_param_kinds.as_ref().map(std::vec::Vec::len).unwrap_or(substituted_params.len());

        if expected_arity != node.node.args.len() {
            return Err(CodegenError::UnsupportedNode { span: node.span, node: "call arity mismatch" });
        }

        let mut args = Vec::with_capacity(node.node.args.len());
        if let Some(kinds) = builtin_param_kinds.as_ref() {
            let mut typed_index = 0usize;
            for (arg, kind) in node.node.args.iter().zip(kinds.iter()) {
                let mut value = lower_node(arg, ctx)?
                    .ok_or(CodegenError::UnsupportedNode { span: arg.span, node: "unit-valued call argument" })?;
                if !matches!(kind, BuiltinType::Ptr) {
                    let expected = substituted_params.get(typed_index).ok_or(CodegenError::UnsupportedNode {
                        span: arg.span,
                        node: "typed builtin parameter mismatch",
                    })?;
                    let actual = ctx.require_expr_type_for_node(arg).unwrap_or(*expected);
                    value = ensure_type_compatibility_or_expected(
                        arg.span,
                        *expected,
                        actual,
                        ctx.type_result,
                        ctx.resolution,
                        ctx.builder,
                        value,
                    )?;
                    typed_index += 1;
                }
                args.push(value);
            }
        } else {
            for (arg, expected) in node.node.args.iter().zip(substituted_params.iter()) {
                let value = if let Some(fn_value) = lower_function_typed_argument(arg, *expected, ctx)? {
                    fn_value
                } else {
                    let mut value = lower_node(arg, ctx)?
                        .ok_or(CodegenError::UnsupportedNode { span: arg.span, node: "unit-valued call argument" })?;
                    let mut actual = ctx.require_expr_type_for_node(arg).unwrap_or(*expected);
                    if let Some(expected_clif) = map_type_id_to_clif(ctx.type_result, *expected) {
                        let value_ty = ctx.builder.func.dfg.value_type(value);
                        if value_ty.is_int() && expected_clif.is_int() && value_ty != expected_clif {
                            if value_ty.bits() < expected_clif.bits() {
                                value = ctx.builder.ins().sextend(expected_clif, value);
                                actual = *expected;
                            } else if value_ty.bits() > expected_clif.bits() {
                                value = ctx.builder.ins().ireduce(expected_clif, value);
                                actual = *expected;
                            }
                        }
                    }
                    ensure_type_compatibility_or_expected(
                        arg.span,
                        *expected,
                        actual,
                        ctx.type_result,
                        ctx.resolution,
                        ctx.builder,
                        value,
                    )?
                };
                args.push(value);
            }
        }

        let mut signature_ir = Signature::new(CallConv::SystemV);
        if let Some(kinds) = builtin_param_kinds.as_ref() {
            let mut typed_index = 0usize;
            for kind in kinds {
                let clif_ty = match kind {
                    BuiltinType::Ptr => pointer_type(),
                    BuiltinType::String => pointer_type(),
                    BuiltinType::Usize | BuiltinType::U64 => types::I64,
                    BuiltinType::F64 => types::F64,
                    BuiltinType::Unit | BuiltinType::Never => {
                        return Err(CodegenError::UnsupportedNode {
                            span: node.span,
                            node: "invalid builtin parameter type",
                        });
                    }
                };
                if !matches!(kind, BuiltinType::Ptr) {
                    let _ = substituted_params.get(typed_index).ok_or(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "typed builtin parameter mismatch",
                    })?;
                    typed_index += 1;
                }
                signature_ir.params.push(AbiParam::new(clif_ty));
            }
        } else {
            for param in &substituted_params {
                let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
                    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call parameter type" })?;
                signature_ir.params.push(AbiParam::new(clif_ty));
            }
        }

        let returns_value = type_returns_runtime_value(ctx.type_result, substituted_return);
        if returns_value {
            let clif_ty = map_type_id_to_clif(ctx.type_result, substituted_return)
                .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call return type" })?;
            signature_ir.returns.push(AbiParam::new(clif_ty));
        }

        let is_builtin = ctx.resolution.builtin_items.contains_key(&item_id);
        let name = if let Some(index) = ctx.resolution.builtin_items.get(&item_id) {
            builtin_specs()
                .get(*index)
                .map(|spec| spec.runtime_symbol.to_string())
                .ok_or(CodegenError::MissingSymbol("builtin symbol"))?
        } else {
            let item_info = ctx.resolution.items.get(item_id.0).ok_or(CodegenError::MissingSymbol("function item"))?;
            let symbol_name = if !generic_args.is_empty() {
                let key = crate::lowering::context::MonomorphKey { item: item_id, args: generic_args.clone() };
                if let Some(existing) = ctx.codegen.monomorphized_functions.get(&key) {
                    existing.clone()
                } else {
                    let def =
                        ctx.function_defs.get(&item_id).ok_or(CodegenError::MissingSymbol("function definition"))?;
                    let base = item_info.name.rsplit("::").next().unwrap_or(&item_info.name);
                    let mangled =
                        mangle_generic_item_function(item_id, base, &generic_args, ctx.resolution, ctx.type_result);
                    if ctx.codegen.symbol_emitted(&mangled) {
                        ctx.codegen.monomorphized_functions.insert(key, mangled.clone());
                        mangled
                    } else if ctx.codegen.emitting_items.contains(&item_id) {
                        mangled
                    } else {
                        let saved_source_path = ctx.codegen.current_source_path.clone();
                        ctx.codegen.current_source_path = ctx
                            .resolution
                            .items
                            .get(item_id.0)
                            .and_then(|info| info.source_path.clone())
                            .or_else(|| saved_source_path.clone());
                        let lower_result = lower_function_with_name(
                            def,
                            ctx.resolution,
                            ctx.type_result,
                            ctx.function_defs,
                            ctx.codegen,
                            Some(mangled.clone()),
                            Some(mapping.clone()),
                            Some(item_id),
                        );
                        ctx.codegen.current_source_path = saved_source_path;
                        lower_result?;
                        ctx.codegen.monomorphized_functions.insert(key, mangled.clone());
                        mangled
                    }
                }
            } else if item_info.kind == ItemKind::Method {
                if let Some((receiver, method)) = item_info.name.rsplit_once("::") {
                    let receiver_short = receiver.rsplit("::").next().unwrap_or(receiver);
                    mangle_method_name(receiver_short, method)
                } else {
                    item_info.name.clone()
                }
            } else {
                let symbol_name = mangle_item_function(ctx.resolution, item_id);
                if !ctx.codegen.symbol_emitted(&symbol_name) && !ctx.codegen.emitting_items.contains(&item_id) {
                    let saved_source_path = ctx.codegen.current_source_path.clone();
                    ctx.codegen.current_source_path = ctx
                        .resolution
                        .items
                        .get(item_id.0)
                        .and_then(|info| info.source_path.clone())
                        .or_else(|| saved_source_path.clone());
                    let lower_result = if let Some(def) = ctx.function_defs.get(&item_id) {
                        lower_function_with_name(
                            def,
                            ctx.resolution,
                            ctx.type_result,
                            ctx.function_defs,
                            ctx.codegen,
                            None,
                            None,
                            Some(item_id),
                        )
                    } else if let (Some(info), Some(hir)) = (
                        ctx.resolution.items.get(item_id.0),
                        crate::linking::load_hir_program_for_item(ctx.resolution, item_id),
                    ) && let Some(def) = {
                        let short_name = info.name.rsplit("::").next().unwrap_or(&info.name);
                        crate::linking::find_function_by_span(&hir, info.span)
                            .or_else(|| crate::linking::find_function_by_name(&hir, short_name))
                    } {
                        lower_function_with_name(
                            def,
                            ctx.resolution,
                            ctx.type_result,
                            ctx.function_defs,
                            ctx.codegen,
                            None,
                            None,
                            Some(item_id),
                        )
                    } else {
                        Ok(())
                    };
                    ctx.codegen.current_source_path = saved_source_path;
                    lower_result?;
                }
                symbol_name
            };
            if !ctx.codegen.symbol_emitted(&symbol_name) && !ctx.codegen.emitting_items.contains(&item_id) {
                return Err(CodegenError::VerificationFailed {
                    function: symbol_name.clone(),
                    message: "link plan missing callee".to_string(),
                });
            }
            symbol_name
        };

        if is_builtin {
            if name == "range" {
                return Ok(None);
            }
            if name == "str_len" && !args.is_empty() {
                let handle = args[0];
                let len_offset = ctx.builder.ins().iconst(pointer_type(), 8);
                let len_addr = ctx.builder.ins().iadd(handle, len_offset);
                let len = ctx.builder.ins().load(types::I64, MemFlags::new(), len_addr, 0);
                return Ok(Some(len));
            }
        }

        if is_builtin && let Some(route) = dispatch_route_for_symbol(&name) {
            return lower_dispatch_builtin_call(node.span, route, &args, returns_value, ctx);
        }

        let sig_ref = ctx.builder.func.import_signature(signature_ir);
        let func_ref = ctx.builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(name),
            signature: sig_ref,
            colocated: !is_builtin,
            patchable: false,
        });

        let call = ctx.builder.ins().call(func_ref, &args);
        lower_call_return(call, node.span, substituted_return, returns_value, ctx)
    }
}
