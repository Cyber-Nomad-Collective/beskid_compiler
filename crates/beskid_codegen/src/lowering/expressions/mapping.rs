//! AOT object-to-object mapping lowering for eligible `[Serialize]` struct pairs.

use crate::errors::CodegenError;
use crate::lowering::expressions::dynamic::emit_dynamic_map_aot;
use crate::lowering::expressions::serialize::require_mapping_eligible;
use beskid_analysis::resolve::ItemId;
use beskid_analysis::resolve::Resolution;
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::TypeResult;
use cranelift_codegen::ir::Value;
use cranelift_frontend::FunctionBuilder;

/// Shape id placeholder until analysis threads mod-generated shape tables into codegen.
pub fn shape_id_for_item(item_id: ItemId) -> u32 {
    // FNV-1a 32-bit — stable per item id for v0.3 tests and AOT tables.
    let mut hash: u32 = 2_166_136_261;
    for byte in item_id.0.to_le_bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

/// Emit an AOT mapping call after eligibility checks; returns CLIF `i32` status value.
#[allow(clippy::too_many_arguments)]
pub fn lower_aot_object_mapping(
    builder: &mut FunctionBuilder,
    resolution: &Resolution,
    type_result: &TypeResult,
    span: SpanInfo,
    src_item: ItemId,
    dst_item: ItemId,
    src_ptr: Value,
    dst_out: Value,
) -> Result<Value, CodegenError> {
    require_mapping_eligible(span, resolution, type_result, src_item, dst_item)?;
    let src_shape = shape_id_for_item(src_item);
    let dst_shape = shape_id_for_item(dst_item);
    Ok(emit_dynamic_map_aot(
        builder,
        src_shape,
        dst_shape,
        src_ptr,
        dst_out,
    ))
}

#[cfg(test)]
mod dynamic_mapping_tests {
    use super::*;
    use crate::lowering::types::pointer_type;
    use beskid_analysis::resolve::ItemId;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
    use cranelift_codegen::isa::CallConv;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

    #[test]
    fn dynamic_shape_id_is_deterministic_per_item() {
        let a = ItemId(3);
        let b = ItemId(3);
        let c = ItemId(4);
        assert_eq!(shape_id_for_item(a), shape_id_for_item(b));
        assert_ne!(shape_id_for_item(a), shape_id_for_item(c));
    }

    #[test]
    fn dynamic_aot_mapping_emits_dynamic_map_aot_call() {
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(pointer_type()));
        sig.params.push(AbiParam::new(pointer_type()));
        sig.returns.push(AbiParam::new(pointer_type()));
        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::testcase("dynamic_mapping_test"),
            sig,
        );
        let mut fn_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let src_ptr = builder.ins().iconst(pointer_type(), 0);
        let dst_out = builder.ins().iconst(pointer_type(), 0);
        let _status = emit_dynamic_map_aot(
            &mut builder,
            shape_id_for_item(ItemId(1)),
            shape_id_for_item(ItemId(2)),
            src_ptr,
            dst_out,
        );

        builder.ins().return_(&[src_ptr]);
        builder.finalize();

        let clif = func.to_string();
        assert!(
            clif.contains("dynamic_map_aot"),
            "expected dynamic_map_aot import in CLIF: {clif}"
        );
    }
}
