//! Lower soft runtime builtins through v3 interop dispatch envelopes.

use crate::errors::CodegenError;
use crate::lowering::node_context::NodeLoweringContext;
use beskid_abi::DispatchRoute;
use beskid_analysis::syntax::SpanInfo;
use cranelift_codegen::ir::Value;
use cranelift_frontend::FunctionBuilder;

pub(crate) fn lower_dispatch_builtin_call(
    span: SpanInfo,
    route: DispatchRoute,
    args: &[Value],
    returns_value: bool,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    beskid_isle::emit_dispatch_call(ctx.builder, route, args, returns_value)
        .map_err(|node| CodegenError::UnsupportedNode { span, node })
}

pub(crate) fn emit_dispatch_call(
    builder: &mut FunctionBuilder,
    route: DispatchRoute,
    args: &[Value],
    returns_value: bool,
) -> Result<Option<Value>, &'static str> {
    beskid_isle::emit_dispatch_call(builder, route, args, returns_value)
}

pub(crate) fn emit_str_from_i64_dispatch(builder: &mut FunctionBuilder, value: Value) -> Result<Value, &'static str> {
    beskid_isle::emit_str_from_i64_dispatch(builder, value)
}
