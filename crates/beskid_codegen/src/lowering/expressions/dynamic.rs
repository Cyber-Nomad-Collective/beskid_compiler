//! CLIF lowering helpers for the v0.3 `dynamic` cell surface.

use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_abi::{
    SYM_DYNAMIC_CAST_CHECKED, SYM_DYNAMIC_CELL_CREATE, SYM_DYNAMIC_CELL_WRAP, SYM_DYNAMIC_MAP_AOT,
    SYM_DYNAMIC_MAP_FALLBACK,
};
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, InstBuilder, Signature, Value, types,
};
use cranelift_codegen::isa::CallConv;

fn import_builtin(
    ctx: &mut NodeLoweringContext<'_, '_>,
    symbol: &str,
    params: &[cranelift_codegen::ir::Type],
    returns: Option<cranelift_codegen::ir::Type>,
) -> cranelift_codegen::ir::FuncRef {
    let mut sig = Signature::new(CallConv::SystemV);
    for param in params {
        sig.params.push(AbiParam::new(*param));
    }
    if let Some(ret) = returns {
        sig.returns.push(AbiParam::new(ret));
    }
    let sig_ref = ctx.builder.func.import_signature(sig);
    ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(symbol),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    })
}

/// Emit `dynamic_cell_create(shape_id, payload)` → `*DynamicCell`.
pub fn emit_dynamic_cell_create(
    ctx: &mut NodeLoweringContext<'_, '_>,
    shape_id: u32,
    payload: Value,
) -> Value {
    let func_ref = import_builtin(
        ctx,
        SYM_DYNAMIC_CELL_CREATE,
        &[types::I64, pointer_type()],
        Some(pointer_type()),
    );
    let shape_val = ctx.builder.ins().iconst(types::I64, i64::from(shape_id));
    let call = ctx.builder.ins().call(func_ref, &[shape_val, payload]);
    ctx.builder.inst_results(call)[0]
}

/// Emit `dynamic_cell_wrap(shape_id, static_ptr)` → `*DynamicCell`.
pub fn emit_dynamic_cell_wrap(
    ctx: &mut NodeLoweringContext<'_, '_>,
    shape_id: u32,
    static_ptr: Value,
) -> Value {
    let func_ref = import_builtin(
        ctx,
        SYM_DYNAMIC_CELL_WRAP,
        &[types::I64, pointer_type()],
        Some(pointer_type()),
    );
    let shape_val = ctx.builder.ins().iconst(types::I64, i64::from(shape_id));
    let call = ctx.builder.ins().call(func_ref, &[shape_val, static_ptr]);
    ctx.builder.inst_results(call)[0]
}

/// Emit `dynamic_cast_checked(cell, expected_shape)` → `i32` status.
pub fn emit_dynamic_cast_checked(
    ctx: &mut NodeLoweringContext<'_, '_>,
    cell: Value,
    expected_shape: u32,
) -> Value {
    let func_ref = import_builtin(
        ctx,
        SYM_DYNAMIC_CAST_CHECKED,
        &[pointer_type(), types::I64],
        Some(types::I32),
    );
    let shape_val = ctx
        .builder
        .ins()
        .iconst(types::I64, i64::from(expected_shape));
    let call = ctx.builder.ins().call(func_ref, &[cell, shape_val]);
    ctx.builder.inst_results(call)[0]
}

/// Emit `dynamic_map_aot(src_shape, dst_shape, src_ptr, dst_out)` → `i32` status.
pub fn emit_dynamic_map_aot(
    ctx: &mut NodeLoweringContext<'_, '_>,
    src_shape: u32,
    dst_shape: u32,
    src_ptr: Value,
    dst_out: Value,
) -> Value {
    let func_ref = import_builtin(
        ctx,
        SYM_DYNAMIC_MAP_AOT,
        &[types::I64, types::I64, pointer_type(), pointer_type()],
        Some(types::I32),
    );
    let src_shape_val = ctx.builder.ins().iconst(types::I64, i64::from(src_shape));
    let dst_shape_val = ctx.builder.ins().iconst(types::I64, i64::from(dst_shape));
    let call = ctx
        .builder
        .ins()
        .call(func_ref, &[src_shape_val, dst_shape_val, src_ptr, dst_out]);
    ctx.builder.inst_results(call)[0]
}

/// Emit `dynamic_map_fallback(cell, dst_shape, dst_out)` → `i32` status.
pub fn emit_dynamic_map_fallback(
    ctx: &mut NodeLoweringContext<'_, '_>,
    cell: Value,
    dst_shape: u32,
    dst_out: Value,
) -> Value {
    let func_ref = import_builtin(
        ctx,
        SYM_DYNAMIC_MAP_FALLBACK,
        &[pointer_type(), types::I64, pointer_type()],
        Some(types::I32),
    );
    let dst_shape_val = ctx.builder.ins().iconst(types::I64, i64::from(dst_shape));
    let call = ctx
        .builder
        .ins()
        .call(func_ref, &[cell, dst_shape_val, dst_out]);
    ctx.builder.inst_results(call)[0]
}

#[cfg(test)]
mod dynamic_clif_tests {
    use super::*;
    use crate::lowering::context::CodegenContext;
    use crate::lowering::function::FunctionLoweringState;
    use crate::lowering::node_context::NodeLoweringContext;
    use beskid_analysis::resolve::Resolution;
    use beskid_analysis::resolve::module_graph::ModuleGraph;
    use beskid_analysis::types::{TypeResult, TypeTable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use std::collections::HashMap;

    fn empty_type_result() -> TypeResult {
        TypeResult {
            types: TypeTable::new(),
            named_type_names: HashMap::new(),
            expr_types: HashMap::new(),
            scoped_expr_types: HashMap::new(),
            local_types: HashMap::new(),
            function_signatures: HashMap::new(),
            method_function_signatures: HashMap::new(),
            struct_fields_ordered: HashMap::new(),
            struct_event_fields: HashMap::new(),
            enum_variants_ordered: HashMap::new(),
            generic_items: HashMap::new(),
            call_kinds: HashMap::new(),
            scoped_call_kinds: HashMap::new(),
            contract_method_order: HashMap::new(),
            contract_signatures: HashMap::new(),
            cast_intents: Vec::new(),
        }
    }

    #[test]
    fn dynamic_cell_create_emits_builtin_call() {
        let type_result = empty_type_result();
        let resolution = Resolution {
            items: Vec::new(),
            module_graph: ModuleGraph::new_root(),
            tables: Default::default(),
            warnings: Vec::new(),
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: Default::default(),
            by_symbol: HashMap::new(),
        };
        let function_defs = HashMap::new();
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(pointer_type()));
        sig.returns.push(AbiParam::new(pointer_type()));
        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::testcase("dynamic_test"),
            sig,
        );
        let mut fn_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut state = FunctionLoweringState::default();
        let mut codegen = CodegenContext::new();
        let null_payload = builder.ins().iconst(pointer_type(), 0);
        {
            let mut ctx = NodeLoweringContext {
                resolution: &resolution,
                type_result: &type_result,
                codegen: &mut codegen,
                function_defs: &function_defs,
                builder: &mut builder,
                state: &mut state,
                expected_return_type: None,
                receiver_type: None,
            };
            let _cell = emit_dynamic_cell_create(&mut ctx, 7, null_payload);
            ctx.builder.ins().return_(&[null_payload]);
        }
        builder.finalize();

        let clif = func.to_string();
        assert!(
            clif.contains("dynamic_cell_create"),
            "expected dynamic_cell_create import in CLIF: {clif}"
        );
    }
}
