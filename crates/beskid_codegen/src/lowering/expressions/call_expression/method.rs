use super::common::{lower_call_return, type_returns_runtime_value};
use super::lambda::lower_function_typed_argument;
use super::{
    AbiParam, CallConv, CodegenError, ExtFuncData, ExternalName, HirCallExpression, HirExpressionNode, InstBuilder,
    MethodReceiverSource, NodeLoweringContext, Signature, SpanInfo, Spanned, TypeId, TypeInfo, Value,
    canonical_item_id, ensure_type_compatibility_or_expected, lower_node, mangle_method_name, map_type_id_to_clif,
    pointer_type,
};

fn receiver_and_method_name(
    method_item_id: beskid_analysis::resolve::ItemId,
    receiver_type: TypeId,
    ctx: &NodeLoweringContext<'_, '_>,
) -> Result<(String, String), CodegenError> {
    let full_name = ctx
        .resolution
        .items
        .iter()
        .find(|info| info.id == method_item_id)
        .map(|info| info.name.clone())
        .ok_or(CodegenError::MissingSymbol("method item"))?;
    if let Some((receiver, method)) = full_name.rsplit_once("::") {
        let receiver_short = receiver.rsplit("::").next().unwrap_or(receiver);
        return Ok((receiver_short.to_string(), method.to_string()));
    }
    let receiver_name = method_receiver_display_name(ctx, receiver_type)?;
    Ok((receiver_name, full_name))
}

fn method_receiver_display_name(
    ctx: &NodeLoweringContext<'_, '_>,
    receiver_type: TypeId,
) -> Result<String, CodegenError> {
    let item_id = match ctx.type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => {
            return Err(CodegenError::UnsupportedNode { span: SpanInfo::default(), node: "method receiver type" });
        }
    };
    let name = ctx
        .resolution
        .items
        .get(item_id.0)
        .map(|info| info.name.as_str())
        .ok_or(CodegenError::MissingSymbol("method receiver item"))?;
    Ok(name.rsplit("::").next().unwrap_or(name).to_string())
}

pub(super) fn lower_method_dispatch_call(
    node: &Spanned<HirCallExpression>,
    method_item_id: beskid_analysis::resolve::ItemId,
    receiver_source: MethodReceiverSource,
    receiver_type: TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let method_item_id = canonical_item_id(ctx.resolution, method_item_id);
    let signature = ctx
        .type_result
        .method_function_signatures
        .get(&method_item_id)
        .or_else(|| ctx.type_result.function_signatures.get(&method_item_id))
        .ok_or(CodegenError::MissingSymbol("method signature"))?;

    if signature.params.len() != node.node.args.len() {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "call arity mismatch" });
    }

    let receiver_value = match receiver_source {
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
                    node: "method receiver source",
                });
            };
            if member_expr.node.target.span != span {
                return Err(CodegenError::UnsupportedNode {
                    span: node.node.callee.span,
                    node: "method receiver span mismatch",
                });
            }
            lower_node(&member_expr.node.target, ctx)?.ok_or(CodegenError::UnsupportedNode {
                span: member_expr.node.target.span,
                node: "unit-valued method receiver",
            })?
        }
    };

    let mut args = Vec::with_capacity(node.node.args.len() + 1);
    args.push(receiver_value);
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
    let receiver_clif_ty = map_type_id_to_clif(ctx.type_result, receiver_type)
        .or_else(|| match ctx.type_result.types.get(receiver_type) {
            Some(TypeInfo::Named(_) | TypeInfo::Applied { .. }) => Some(pointer_type()),
            _ => None,
        })
        .ok_or(CodegenError::UnsupportedNode { span: node.node.callee.span, node: "method receiver type" })?;
    signature_ir.params.push(AbiParam::new(receiver_clif_ty));
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

    let (receiver_name, method_name) = receiver_and_method_name(method_item_id, receiver_type, ctx)?;
    let function_name = mangle_method_name(&receiver_name, &method_name);
    let sig_ref = ctx.builder.func.import_signature(signature_ir);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(function_name),
        signature: sig_ref,
        colocated: true,
        patchable: false,
    });

    let call = ctx.builder.ins().call(func_ref, &args);
    lower_call_return(call, node.span, signature.return_type, returns_value, ctx)
}
