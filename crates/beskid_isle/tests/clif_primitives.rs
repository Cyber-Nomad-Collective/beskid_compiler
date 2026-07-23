use beskid_isle::ClifPrimitives;
use cranelift_codegen::ir::{Function, InstBuilder, types};
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

#[test]
fn clif_primitives_extending_loads_and_unsigned_compare_verify() {
    let mut function = Function::new();
    function.signature.params.push(cranelift_codegen::ir::AbiParam::new(types::I64));
    function.signature.returns.push(cranelift_codegen::ir::AbiParam::new(types::I64));
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        let base = builder.block_params(block)[0];
        let mut primitives = ClifPrimitives::new(&mut builder);
        let byte = primitives.load_i8_zext(base, 0).expect("i8 zext");
        let half = primitives.load_i16_zext(base, 2).expect("i16 zext");
        let word = primitives.load_i32_sext(base, 4).expect("i32 sext");
        let cmp = primitives.icmp_ult(byte, half);
        let summed = primitives.builder_mut().ins().iadd(word, byte);
        let flagged = primitives.builder_mut().ins().uextend(types::I64, cmp);
        let result = primitives.builder_mut().ins().iadd(summed, flagged);
        let stored = primitives.stack_store_i64(result, 0);
        let loaded = primitives.stack_load_i64(0);
        let _ = primitives.icmp_eq(stored, loaded);
        let f = primitives.fcvt_from_sint_f64(loaded);
        let neg = primitives.fneg_f64(f);
        let back = primitives.fcvt_to_sint_i64(neg);
        builder.ins().return_(&[back]);
        builder.finalize();
    }

    verify_function(
        &function,
        &cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()),
    )
    .expect("primitives CLIF verifies");
    let clif = function.display().to_string();
    assert!(clif.contains("uextend"), "{clif}");
    assert!(clif.contains("ult") || clif.contains("icmp ult"), "{clif}");
    assert!(clif.contains("stack_load") || clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("fcvt"), "{clif}");
}
