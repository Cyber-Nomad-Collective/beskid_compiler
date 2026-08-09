use super::common::{lower_call_return, type_returns_runtime_value};
use super::lambda::lower_function_typed_argument;
use super::{
    AbiParam, CallConv, CodegenError, ExtFuncData, ExternalName, HirCallExpression, HirExpressionNode, InstBuilder,
    MemFlags, MethodReceiverSource, NodeLoweringContext, ResolvedValue, Signature, Spanned, TypeInfo, Value,
    contract_method_order, contract_signatures, ensure_type_compatibility_or_expected, lower_node, map_type_id_to_clif,
    method_name_from_path_callee, pointer_type, resolved_value_at,
};

fn contract_method_name(callee: &Spanned<HirExpressionNode>) -> Option<String> {
    match &callee.node {
        HirExpressionNode::PathExpression(path_expr) => {
            method_name_from_path_callee(&path_expr.node.path.node.segments).map(str::to_string)
        }
        HirExpressionNode::MemberExpression(member_expr) => Some(member_expr.node.member.node.name.clone()),
        _ => None,
    }
}

pub(super) fn lower_contract_dispatch_call(
    node: &Spanned<HirCallExpression>,
    contract_item_id: beskid_analysis::resolve::ItemId,
    receiver_source: MethodReceiverSource,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let method_name = contract_method_name(&node.node.callee)
        .ok_or(CodegenError::UnsupportedNode { span: node.node.callee.span, node: "contract dispatch callee" })?;
    let contract_orders = contract_method_order(ctx.type_result);
    let method_order =
        contract_orders.get(&contract_item_id).ok_or(CodegenError::MissingSymbol("contract method order"))?;
    let method_index = method_order
        .iter()
        .position(|name| name == &method_name)
        .ok_or(CodegenError::MissingSymbol("contract method slot"))?;
    let contract_sigs = contract_signatures(ctx.type_result);
    let signature = contract_sigs
        .get(&(contract_item_id, method_name.clone()))
        .ok_or(CodegenError::MissingSymbol("contract method signature"))?;

    if signature.params.len() != node.node.args.len() {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "call arity mismatch" });
    }

    // Special-case: language-level extern contract call such as `C.getpid(...)`.
    // If the callee target resolves to an Item (contract type) rather than an instance wrapper,
    // emit a direct external call with no implicit receiver argument.
    if let HirExpressionNode::MemberExpression(member_expr) = &node.node.callee.node
        && let HirExpressionNode::PathExpression(path) = &member_expr.node.target.node
        && let Some(resolved) =
            resolved_value_at(ctx.resolution, path.node.path.span, ctx.codegen.current_source_path.as_ref())
        && matches!(resolved, ResolvedValue::Item(item_id) if item_id == contract_item_id)
    {
        // Direct extern call: build args from call site only, no receiver wrapper.
        if signature.params.len() != node.node.args.len() {
            return Err(CodegenError::UnsupportedNode { span: node.span, node: "call arity mismatch" });
        }

        let mut args = Vec::with_capacity(node.node.args.len());
        for (arg, expected) in node.node.args.iter().zip(signature.params.iter()) {
            let value = if let Some(fn_value) = lower_function_typed_argument(arg, *expected, ctx)? {
                fn_value
            } else {
                let lowered = lower_node(arg, ctx)?
                    .ok_or(CodegenError::UnsupportedNode { span: arg.span, node: "unit-valued call argument" })?;
                let actual = ctx.require_expr_type_for_node(arg).unwrap_or(*expected);
                ensure_type_compatibility_or_expected(
                    arg.span,
                    *expected,
                    actual,
                    ctx.type_result,
                    ctx.resolution,
                    ctx.builder,
                    lowered,
                )?
            };
            args.push(value);
        }

        let mut signature_ir = Signature::new(CallConv::SystemV);
        for param in &signature.params {
            let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
                .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call parameter type" })?;
            signature_ir.params.push(AbiParam::new(clif_ty));
        }

        let returns_value = type_returns_runtime_value(ctx.type_result, signature.return_type);
        if returns_value {
            let clif_ty = map_type_id_to_clif(ctx.type_result, signature.return_type)
                .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call return type" })?;
            signature_ir.returns.push(AbiParam::new(clif_ty));
        }

        let sig_ref = ctx.builder.func.import_signature(signature_ir);
        let func_ref = ctx.builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(method_name.clone()),
            signature: sig_ref,
            colocated: true,
            patchable: false,
        });
        let call = ctx.builder.ins().call(func_ref, &args);
        return lower_call_return(call, node.span, signature.return_type, returns_value, ctx);
    }
    // Also support the dotted PathExpression form emitted by the frontend for `C.getpid(...)`.
    if let HirExpressionNode::PathExpression(path_expr) = &node.node.callee.node {
        // Expect at least two segments: C.getpid
        if path_expr.node.path.node.segments.len() >= 2
            && let Some(ResolvedValue::Item(item_id)) =
                resolved_value_at(ctx.resolution, path_expr.node.path.span, ctx.codegen.current_source_path.as_ref())
            && item_id == contract_item_id
        {
            // Build direct extern call with method_name
            let mut args = Vec::with_capacity(node.node.args.len());
            for (arg, expected) in node.node.args.iter().zip(signature.params.iter()) {
                let value = if let Some(fn_value) = lower_function_typed_argument(arg, *expected, ctx)? {
                    fn_value
                } else {
                    let lowered = lower_node(arg, ctx)?
                        .ok_or(CodegenError::UnsupportedNode { span: arg.span, node: "unit-valued call argument" })?;
                    let actual = ctx.require_expr_type_for_node(arg).unwrap_or(*expected);
                    ensure_type_compatibility_or_expected(
                        arg.span,
                        *expected,
                        actual,
                        ctx.type_result,
                        ctx.resolution,
                        ctx.builder,
                        lowered,
                    )?
                };
                args.push(value);
            }

            let mut signature_ir = Signature::new(CallConv::SystemV);
            for param in &signature.params {
                let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
                    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call parameter type" })?;
                signature_ir.params.push(AbiParam::new(clif_ty));
            }

            let returns_value = type_returns_runtime_value(ctx.type_result, signature.return_type);
            if returns_value {
                let clif_ty = map_type_id_to_clif(ctx.type_result, signature.return_type)
                    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call return type" })?;
                signature_ir.returns.push(AbiParam::new(clif_ty));
            }

            let sig_ref = ctx.builder.func.import_signature(signature_ir);
            let func_ref = ctx.builder.func.import_function(ExtFuncData {
                name: ExternalName::testcase(method_name.clone()),
                signature: sig_ref,
                colocated: true,
                patchable: false,
            });
            let call = ctx.builder.ins().call(func_ref, &args);
            return lower_call_return(call, node.span, signature.return_type, returns_value, ctx);
        }
    }

    let receiver_wrapper = match receiver_source {
        MethodReceiverSource::Local(local_id) => {
            let receiver_var = ctx
                .state
                .locals
                .get(&local_id)
                .copied()
                .ok_or(CodegenError::InvalidLocalBinding { span: node.node.callee.span })?;
            ctx.builder.use_var(receiver_var)
        }
        MethodReceiverSource::Expression(span) => {
            let HirExpressionNode::MemberExpression(member_expr) = &node.node.callee.node else {
                return Err(CodegenError::UnsupportedNode {
                    span: node.node.callee.span,
                    node: "contract receiver source",
                });
            };
            if member_expr.node.target.span != span {
                return Err(CodegenError::UnsupportedNode {
                    span: node.node.callee.span,
                    node: "contract receiver span mismatch",
                });
            }
            lower_node(&member_expr.node.target, ctx)?.ok_or(CodegenError::UnsupportedNode {
                span: member_expr.node.target.span,
                node: "unit-valued contract receiver",
            })?
        }
    };

    let data_ptr = ctx.builder.ins().load(pointer_type(), MemFlags::new(), receiver_wrapper, 0);
    let method_offset = ((method_index + 1) * std::mem::size_of::<u64>()) as i32;
    let method_ptr = ctx.builder.ins().load(pointer_type(), MemFlags::new(), receiver_wrapper, method_offset);

    let mut args = Vec::with_capacity(node.node.args.len() + 1);
    args.push(data_ptr);
    for (arg, expected) in node.node.args.iter().zip(signature.params.iter()) {
        let value = if let Some(fn_value) = lower_function_typed_argument(arg, *expected, ctx)? {
            fn_value
        } else {
            let lowered = lower_node(arg, ctx)?
                .ok_or(CodegenError::UnsupportedNode { span: arg.span, node: "unit-valued call argument" })?;
            let actual = ctx.require_expr_type_for_node(arg).unwrap_or(*expected);
            ensure_type_compatibility_or_expected(
                arg.span,
                *expected,
                actual,
                ctx.type_result,
                ctx.resolution,
                ctx.builder,
                lowered,
            )?
        };
        args.push(value);
    }

    let mut signature_ir = Signature::new(CallConv::SystemV);
    signature_ir.params.push(AbiParam::new(pointer_type()));
    for param in &signature.params {
        let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
            .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call parameter type" })?;
        signature_ir.params.push(AbiParam::new(clif_ty));
    }

    let returns_value = type_returns_runtime_value(ctx.type_result, signature.return_type);
    if returns_value {
        let clif_ty = map_type_id_to_clif(ctx.type_result, signature.return_type)
            .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "call return type" })?;
        signature_ir.returns.push(AbiParam::new(clif_ty));
    }

    let sig_ref = ctx.builder.func.import_signature(signature_ir);
    let call = ctx.builder.ins().call_indirect(sig_ref, method_ptr, &args);
    lower_call_return(call, node.span, signature.return_type, returns_value, ctx)
}
