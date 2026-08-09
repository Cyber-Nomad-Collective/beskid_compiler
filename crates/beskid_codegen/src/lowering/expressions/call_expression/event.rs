use crate::lowering::cast_intent::ensure_type_compatibility_or_expected;
use beskid_analysis::hir::HirPrimitiveType;

use super::lambda::lower_function_typed_argument;
use super::{
    AbiParam, CallConv, CodegenError, DispatchReturnGroup, DispatchRoute, HirCallExpression, HirExpressionNode,
    InstBuilder, IntCC, MemFlags, MethodReceiverSource, NodeLoweringContext, Signature, Spanned, TAG_EVENT_GET_HANDLER,
    TAG_EVENT_LEN, TypeId, TypeInfo, Value, first_field_segment_name, lower_dispatch_builtin_call, lower_node,
    map_type_id_to_clif, pointer_type,
};

fn event_field_name(callee: &Spanned<HirExpressionNode>) -> Option<String> {
    match &callee.node {
        HirExpressionNode::PathExpression(path_expr) => {
            first_field_segment_name(&path_expr.node.path.node.segments).map(str::to_string)
        }
        HirExpressionNode::MemberExpression(member_expr) => Some(member_expr.node.member.node.name.clone()),
        _ => None,
    }
}

pub(super) fn lower_event_invoke_call(
    node: &Spanned<HirCallExpression>,
    receiver_source: MethodReceiverSource,
    receiver_type: TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let field_name = event_field_name(&node.node.callee)
        .ok_or(CodegenError::UnsupportedNode { span: node.node.callee.span, node: "event invoke callee" })?;
    let item_id = match ctx.type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        _ => {
            return Err(CodegenError::UnsupportedNode { span: node.span, node: "event invoke receiver type" });
        }
    };
    let field_type = ctx
        .type_result
        .struct_fields_ordered
        .get(&item_id)
        .and_then(|fields| fields.iter().find(|(name, _)| name == &field_name))
        .map(|(_, ty)| *ty)
        .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event invoke field type" })?;
    let TypeInfo::Function { params, return_type } = ctx
        .type_result
        .types
        .get(field_type)
        .cloned()
        .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event invoke signature" })?
    else {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "event invoke non-function field" });
    };
    if !matches!(ctx.type_result.types.get(return_type), Some(TypeInfo::Primitive(HirPrimitiveType::Unit))) {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "event invoke non-unit return" });
    }
    if params.len() != node.node.args.len() {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "event invoke arity mismatch" });
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
                    node: "event receiver source",
                });
            };
            if member_expr.node.target.span != span {
                return Err(CodegenError::UnsupportedNode {
                    span: node.node.callee.span,
                    node: "event receiver span mismatch",
                });
            }
            lower_node(&member_expr.node.target, ctx)?.ok_or(CodegenError::UnsupportedNode {
                span: member_expr.node.target.span,
                node: "unit-valued event receiver",
            })?
        }
    };

    let offsets = crate::lowering::descriptor::struct_field_offsets(
        ctx.resolution,
        ctx.type_result,
        item_id,
        ctx.codegen.current_source_path.as_ref(),
    )
    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event invoke offsets" })?;
    let offset = offsets
        .get(field_name.as_str())
        .copied()
        .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event invoke field offset" })?;
    let offset_val = ctx.builder.ins().iconst(pointer_type(), offset as i64);
    let field_addr = ctx.builder.ins().iadd(receiver_value, offset_val);
    let event_state = ctx.builder.ins().load(pointer_type(), MemFlags::new(), field_addr, 0);

    let mut lowered_args = Vec::with_capacity(params.len());
    for (arg, expected) in node.node.args.iter().zip(params.iter()) {
        let value = if let Some(fn_value) = lower_function_typed_argument(arg, *expected, ctx)? {
            fn_value
        } else {
            let lowered = lower_node(arg, ctx)?
                .ok_or(CodegenError::UnsupportedNode { span: arg.span, node: "unit-valued event argument" })?;
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
        lowered_args.push(value);
    }

    let zero = ctx.builder.ins().iconst(pointer_type(), 0);
    let state_is_null = ctx.builder.ins().icmp(IntCC::Equal, event_state, zero);
    let loop_header = ctx.builder.create_block();
    let loop_body = ctx.builder.create_block();
    let loop_exit = ctx.builder.create_block();
    let idx_var = ctx.builder.declare_var(pointer_type());
    ctx.builder.def_var(idx_var, zero);
    ctx.builder.ins().brif(state_is_null, loop_exit, &[], loop_header, &[]);

    ctx.builder.switch_to_block(loop_header);
    let count = lower_dispatch_builtin_call(
        node.span,
        DispatchRoute { tag: TAG_EVENT_LEN, group: DispatchReturnGroup::I64 },
        &[event_state],
        true,
        ctx,
    )?
    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event len result" })?;
    let idx = ctx.builder.use_var(idx_var);
    let done = ctx.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx, count);
    ctx.builder.ins().brif(done, loop_exit, &[], loop_body, &[]);

    ctx.builder.switch_to_block(loop_body);
    let handler_ptr = lower_dispatch_builtin_call(
        node.span,
        DispatchRoute { tag: TAG_EVENT_GET_HANDLER, group: DispatchReturnGroup::I64 },
        &[event_state, idx],
        true,
        ctx,
    )?
    .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event handler result" })?;

    let mut handler_sig = Signature::new(CallConv::SystemV);
    for param in &params {
        let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
            .ok_or(CodegenError::UnsupportedNode { span: node.span, node: "event handler parameter type" })?;
        handler_sig.params.push(AbiParam::new(clif_ty));
    }
    let handler_sig_ref = ctx.builder.func.import_signature(handler_sig);
    let _ = ctx.builder.ins().call_indirect(handler_sig_ref, handler_ptr, &lowered_args);

    let next = ctx.builder.ins().iadd_imm(idx, 1);
    ctx.builder.def_var(idx_var, next);
    ctx.builder.ins().jump(loop_header, &[]);

    ctx.builder.switch_to_block(loop_exit);
    ctx.builder.seal_block(loop_header);
    ctx.builder.seal_block(loop_body);
    ctx.builder.seal_block(loop_exit);
    Ok(None)
}
