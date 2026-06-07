//! Lower soft runtime builtins through v3 interop dispatch envelopes.

use crate::errors::CodegenError;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_abi::{
    DISPATCH_PAD_OFFSET, DISPATCH_PAYLOAD_OFFSET, DISPATCH_TAG_OFFSET, DISPATCH_TYPE_DESC_OFFSET,
    DispatchReturnGroup, DispatchRoute, SYM_INTEROP_DISPATCH_I64, SYM_INTEROP_DISPATCH_PTR,
    SYM_INTEROP_DISPATCH_UNIT, SYM_INTEROP_DISPATCH_USIZE, dispatch_route_for_symbol,
};
use beskid_analysis::syntax::SpanInfo;
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, InstBuilder, MemFlags, Signature, StackSlotData,
    StackSlotKind, Value, types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;

pub(crate) fn lower_dispatch_builtin_call(
    span: SpanInfo,
    route: DispatchRoute,
    args: &[Value],
    returns_value: bool,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    emit_dispatch_call(ctx.builder, route, args, returns_value)
        .map_err(|node| CodegenError::UnsupportedNode { span, node })
}

pub(crate) fn emit_dispatch_call(
    builder: &mut FunctionBuilder,
    route: DispatchRoute,
    args: &[Value],
    returns_value: bool,
) -> Result<Option<Value>, &'static str> {
    let payload_bytes = args.len().saturating_mul(8);
    let envelope_size = (DISPATCH_PAYLOAD_OFFSET as usize) + payload_bytes;
    let slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        envelope_size as u32,
        3,
    ));
    let envelope_ptr = builder.ins().stack_addr(pointer_type(), slot, 0);

    let null_desc = builder.ins().iconst(pointer_type(), 0);
    builder.ins().store(
        MemFlags::new(),
        null_desc,
        envelope_ptr,
        DISPATCH_TYPE_DESC_OFFSET,
    );

    let tag_val = builder.ins().iconst(types::I32, i64::from(route.tag));
    builder
        .ins()
        .store(MemFlags::new(), tag_val, envelope_ptr, DISPATCH_TAG_OFFSET);

    let pad_val = builder.ins().iconst(types::I32, 0);
    builder
        .ins()
        .store(MemFlags::new(), pad_val, envelope_ptr, DISPATCH_PAD_OFFSET);

    for (index, arg) in args.iter().enumerate() {
        let offset = DISPATCH_PAYLOAD_OFFSET + (index as i32 * 8);
        builder
            .ins()
            .store(MemFlags::new(), *arg, envelope_ptr, offset);
    }

    let (dispatch_symbol, returns_ptr, returns_i64) = match route.group {
        DispatchReturnGroup::Unit => (SYM_INTEROP_DISPATCH_UNIT, false, false),
        DispatchReturnGroup::Ptr => (SYM_INTEROP_DISPATCH_PTR, true, false),
        DispatchReturnGroup::Usize => (SYM_INTEROP_DISPATCH_USIZE, false, true),
        DispatchReturnGroup::I64 => (SYM_INTEROP_DISPATCH_I64, false, true),
    };

    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    if returns_ptr {
        signature.returns.push(AbiParam::new(pointer_type()));
    } else if returns_i64 && returns_value {
        signature.returns.push(AbiParam::new(types::I64));
    }

    let sig_ref = builder.func.import_signature(signature);
    let func_ref = builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(dispatch_symbol.to_string()),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });

    let call = builder.ins().call(func_ref, &[envelope_ptr]);
    if !returns_value {
        return Ok(None);
    }

    let value = *builder
        .inst_results(call)
        .first()
        .ok_or("dispatch call result")?;

    Ok(Some(value))
}

pub(crate) fn emit_str_from_i64_dispatch(
    builder: &mut FunctionBuilder,
    value: Value,
) -> Result<Value, &'static str> {
    let route = dispatch_route_for_symbol("str_from_i64").ok_or("str_from_i64 dispatch route")?;
    emit_dispatch_call(builder, route, &[value], true)?.ok_or("str_from_i64 result")
}
