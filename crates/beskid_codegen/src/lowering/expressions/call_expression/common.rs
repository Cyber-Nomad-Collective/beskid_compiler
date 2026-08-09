use super::{CodegenError, HirPrimitiveType, InstBuilder, NodeLoweringContext, TrapCode, TypeId, TypeInfo, Value};

fn is_never_type(type_result: &beskid_analysis::types::TypeResult, type_id: TypeId) -> bool {
    matches!(type_result.types.get(type_id), Some(TypeInfo::Primitive(HirPrimitiveType::Never)))
}

fn terminate_never_call(ctx: &mut NodeLoweringContext<'_, '_>) {
    ctx.builder.ins().trap(TrapCode::unwrap_user(1));
    ctx.state.block_terminated = true;
}

pub(super) fn lower_call_return(
    call: cranelift_codegen::ir::Inst,
    span: beskid_analysis::syntax::SpanInfo,
    return_type: TypeId,
    returns_value: bool,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    if !returns_value {
        if is_never_type(ctx.type_result, return_type) {
            terminate_never_call(ctx);
        }
        return Ok(None);
    }

    let value =
        *ctx.builder.inst_results(call).first().ok_or(CodegenError::UnsupportedNode { span, node: "call result" })?;
    Ok(Some(value))
}

pub(crate) fn type_returns_runtime_value(type_result: &beskid_analysis::types::TypeResult, type_id: TypeId) -> bool {
    !matches!(
        type_result.types.get(type_id),
        Some(TypeInfo::Primitive(HirPrimitiveType::Unit | HirPrimitiveType::Never))
    )
}
