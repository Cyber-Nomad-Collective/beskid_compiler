//! CLIF lowering helpers for the v0.3 `dynamic` cell surface.

use crate::lowering::types::pointer_type;
use beskid_abi::{SYM_DYNAMIC_CELL_CREATE, SYM_DYNAMIC_MAP_AOT};
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, InstBuilder, Signature, Value, types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;

fn import_builtin(
    builder: &mut FunctionBuilder,
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
    let sig_ref = builder.func.import_signature(sig);
    builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(symbol),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    })
}

/// Emit `dynamic_cell_create(shape_id, payload)` → `*DynamicCell`.
#[allow(dead_code)] // Spec anchor until dynamic wrap/cast lowering wires this helper.
pub(crate) fn emit_dynamic_cell_create(
    builder: &mut FunctionBuilder,
    shape_id: u32,
    payload: Value,
) -> Value {
    let func_ref = import_builtin(
        builder,
        SYM_DYNAMIC_CELL_CREATE,
        &[types::I64, pointer_type()],
        Some(pointer_type()),
    );
    let shape_val = builder.ins().iconst(types::I64, i64::from(shape_id));
    let call = builder.ins().call(func_ref, &[shape_val, payload]);
    builder.inst_results(call)[0]
}

/// Emit `dynamic_map_aot(src_shape, dst_shape, src_ptr, dst_out)` → `i32` status.
pub(crate) fn emit_dynamic_map_aot(
    builder: &mut FunctionBuilder,
    src_shape: u32,
    dst_shape: u32,
    src_ptr: Value,
    dst_out: Value,
) -> Value {
    let func_ref = import_builtin(
        builder,
        SYM_DYNAMIC_MAP_AOT,
        &[types::I64, types::I64, pointer_type(), pointer_type()],
        Some(types::I32),
    );
    let src_shape_val = builder.ins().iconst(types::I64, i64::from(src_shape));
    let dst_shape_val = builder.ins().iconst(types::I64, i64::from(dst_shape));
    let call = builder
        .ins()
        .call(func_ref, &[src_shape_val, dst_shape_val, src_ptr, dst_out]);
    builder.inst_results(call)[0]
}

#[cfg(test)]
mod dynamic_clif_tests {
    use super::*;
    use crate::lowering::context::CodegenContext;
    use crate::lowering::function::FunctionLoweringState;
    use crate::lowering::node_context::NodeLoweringContext;
    use beskid_analysis::resolve::Resolution;
    use beskid_analysis::resolve::module_graph::ModuleGraph;
    use beskid_analysis::types::{LoweringPrep, TypeResult, TypeTable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use std::collections::HashMap;

    fn empty_type_result() -> TypeResult {
        TypeResult {
            types: TypeTable::new(),
            named_type_names: HashMap::new(),
            node_types: HashMap::new(),
            local_types: HashMap::new(),
            unit_surfaces: HashMap::new(),
            function_signatures: HashMap::new(),
            method_function_signatures: HashMap::new(),
            struct_fields_ordered: HashMap::new(),
            struct_event_fields: HashMap::new(),
            enum_variants_ordered: HashMap::new(),
            generic_items: HashMap::new(),
            lowering: LoweringPrep::default(),
        }
    }

    #[test]
    fn dynamic_cell_create_emits_builtin_call() {
        let type_result = empty_type_result();
        let resolution = Resolution {
            items: Vec::new(),
            module_graph: ModuleGraph::new_root(),
            tables: Default::default(),
            span_index: Default::default(),
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
            let ctx = NodeLoweringContext {
                resolution: &resolution,
                type_result: &type_result,
                codegen: &mut codegen,
                function_defs: &function_defs,
                builder: &mut builder,
                state: &mut state,
                expected_return_type: None,
                receiver_type: None,
                expected_expr_type: None,
            };
            let _cell = emit_dynamic_cell_create(ctx.builder, 7, null_payload);
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
