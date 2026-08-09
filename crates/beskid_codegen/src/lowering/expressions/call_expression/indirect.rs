use super::common::lower_call_return;
use super::lambda::{lambda_signature_from_types, lower_function_typed_argument};
use super::{
    CodegenError, HirCallExpression, InstBuilder, NodeLoweringContext, Spanned, TypeId, TypeInfo, Value,
    ensure_type_compatibility_or_expected, lower_node,
};

pub(super) fn lower_indirect_function_call(
    node: &Spanned<HirCallExpression>,
    local_id: beskid_analysis::resolve::LocalId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let callee_type = ctx
        .type_result
        .local_types
        .get(&local_id)
        .copied()
        .ok_or(CodegenError::MissingLocalType { span: node.node.callee.span })?;

    let TypeInfo::Function { params, return_type } = ctx
        .type_result
        .types
        .get(callee_type)
        .cloned()
        .ok_or(CodegenError::MissingLocalType { span: node.node.callee.span })?
    else {
        return Err(CodegenError::UnsupportedNode {
            span: node.node.callee.span,
            node: "non-function local call target",
        });
    };

    if params.len() != node.node.args.len() {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "call arity mismatch" });
    }

    let callee_var = ctx
        .state
        .locals
        .get(&local_id)
        .copied()
        .ok_or(CodegenError::InvalidLocalBinding { span: node.node.callee.span })?;
    let callee_ptr = ctx.builder.use_var(callee_var);

    lower_indirect_function_call_with_signature(node, callee_ptr, &params, return_type, ctx)
}

pub(super) fn lower_indirect_function_call_with_signature(
    node: &Spanned<HirCallExpression>,
    callee_ptr: Value,
    params: &[TypeId],
    return_type: TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    if params.len() != node.node.args.len() {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "call arity mismatch" });
    }

    let mut args = Vec::with_capacity(node.node.args.len());
    for (arg, expected) in node.node.args.iter().zip(params.iter()) {
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

    let (signature_ir, returns_value) = lambda_signature_from_types(params, return_type, node.span, ctx)?;

    let sig_ref = ctx.builder.func.import_signature(signature_ir);
    let call = ctx.builder.ins().call_indirect(sig_ref, callee_ptr, &args);
    lower_call_return(call, node.span, return_type, returns_value, ctx)
}
