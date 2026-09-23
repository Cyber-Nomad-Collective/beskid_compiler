use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Signature, Type};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;

pub(super) fn import_local(
    builder: &mut FunctionBuilder<'_>,
    symbol: &str,
    params: &[Type],
    result: Option<Type>,
) -> cranelift_codegen::ir::FuncRef {
    import_local_with_call_conv(builder, symbol, params, result, builder.func.signature.call_conv)
}

pub(super) fn import_local_with_call_conv(
    builder: &mut FunctionBuilder<'_>,
    symbol: &str,
    params: &[Type],
    result: Option<Type>,
    call_conv: CallConv,
) -> cranelift_codegen::ir::FuncRef {
    import_with_call_conv(builder, symbol, params, result, call_conv, true)
}

pub(super) fn import_with_call_conv(
    builder: &mut FunctionBuilder<'_>,
    symbol: &str,
    params: &[Type],
    result: Option<Type>,
    call_conv: CallConv,
    colocated: bool,
) -> cranelift_codegen::ir::FuncRef {
    let mut signature = Signature::new(call_conv);
    signature.params.extend(params.iter().copied().map(AbiParam::new));
    if let Some(result) = result {
        signature.returns.push(AbiParam::new(result));
    }
    let signature = builder.func.import_signature(signature);
    builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(symbol.as_bytes()),
        signature,
        colocated,
        patchable: false,
    })
}
