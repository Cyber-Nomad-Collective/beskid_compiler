use beskid_isle::ClifPrimitives;
use cranelift_codegen::ir::{Function, InstBuilder, InstructionData, Opcode, ValueDef, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

fn primitives_function(isa: &dyn TargetIsa) -> Function {
    let mut function = Function::new();
    function.signature.call_conv = isa.default_call_conv();
    function.signature.params.push(cranelift_codegen::ir::AbiParam::new(isa.pointer_type()));
    function.signature.returns.push(cranelift_codegen::ir::AbiParam::new(types::I64));
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        let base = builder.block_params(block)[0];
        let mut primitives = ClifPrimitives::new(&mut builder, isa.frontend_config());
        let byte = primitives.load_i8_zext(base, 0).expect("i8 zext");
        let half = primitives.load_i16_zext(base, 2).expect("i16 zext");
        let word = primitives.load_i32_sext(base, 4).expect("i32 sext");
        let cmp = primitives.icmp_ult(byte, half);
        let summed = primitives.builder_mut().ins().iadd(word, byte);
        let flagged = primitives.builder_mut().ins().uextend(types::I64, cmp);
        let result = primitives.builder_mut().ins().iadd(summed, flagged);
        primitives.store_i64(base, 0, result).expect("trusted store");
        let stored = primitives.stack_store_i64(result, 0);
        let loaded = primitives.stack_load_i64(0);
        let _ = primitives.icmp_eq(stored, loaded);
        let f = primitives.fcvt_from_sint_f64(loaded);
        let neg = primitives.fneg_f64(f);
        let back = primitives.fcvt_to_sint_i64(neg);
        builder.ins().return_(&[back]);
        builder.finalize(isa.frontend_config());
    }

    verify_function(&function, isa.flags()).expect("primitives CLIF verifies");
    function
}

#[test]
fn clif_primitives_extending_loads_and_unsigned_compare_verify() {
    for target in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin"] {
        let isa = cranelift_codegen::isa::lookup(target.parse().expect("test target"))
            .expect("target ISA")
            .finish(cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()))
            .expect("test flags");
        let function = primitives_function(isa.as_ref());
        let clif = function.display().to_string();
        let slots = function.sized_stack_slots.iter().collect::<Vec<_>>();
        assert_eq!(slots.len(), 1, "one reusable scratch slot: {clif}");
        assert_eq!(slots[0].1.size, 8);
        let mut trusted_loads = 0;
        let mut trusted_stores = 0;
        let mut stack_accesses = Vec::new();
        for inst in function.layout.blocks().flat_map(|block| function.layout.block_insts(block)) {
            let data = &function.dfg.insts[inst];
            let Some(flags) = data.memflags_data(&function.dfg) else { continue };
            let (address, value, is_store) = match *data {
                InstructionData::Load { arg, .. } => (arg, function.dfg.first_result(inst), false),
                InstructionData::Store { args, .. } => (args[1], args[0], true),
                _ => panic!("unexpected memory instruction: {clif}"),
            };
            if let ValueDef::Result(address_inst, _) = function.dfg.value_def(address)
                && let InstructionData::StackAddr { stack_slot, offset, .. } = function.dfg.insts[address_inst]
            {
                assert_eq!(stack_slot, slots[0].0);
                assert_eq!(i32::from(offset), 0);
                assert_eq!(function.dfg.value_type(address), isa.pointer_type());
                assert_eq!(function.dfg.value_type(value), types::I64);
                assert!(flags.notrap() && !flags.aligned(), "stack expansion preserves accessible-slot flags");
                stack_accesses.push(is_store);
            } else {
                assert!(flags.notrap() && flags.aligned(), "trusted primitive accesses retain trust: {clif}");
                match data.opcode() {
                    Opcode::Load => trusted_loads += 1,
                    Opcode::Store => trusted_stores += 1,
                    _ => panic!("trusted load or store: {clif}"),
                }
            }
        }
        assert_eq!(trusted_loads, 3);
        assert_eq!(trusted_stores, 1);
        assert_eq!(stack_accesses, [true, false], "same scratch slot is stored then loaded: {clif}");
    }
}

#[test]
fn clif_primitives_stack_round_trip_preserves_computed_value() {
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Linkage, Module, default_libcall_names};
    let mut module = JITModule::new(JITBuilder::new(default_libcall_names()).expect("native JIT"));
    let function = primitives_function(module.isa());
    let entry = module.declare_function("primitive_round_trip", Linkage::Export, &function.signature).expect("entry");
    let mut context = module.make_context();
    context.func = function;
    module.define_function(entry, &mut context).expect("compile primitives");
    module.finalize_definitions().expect("finalize primitives");
    let mut input = u64::from_le_bytes([5, 0, 7, 0, 12, 0, 0, 0]);
    // SAFETY: the JIT signature is native (pointer) -> i64, the aligned input
    // contains eight readable/writable bytes, and the module outlives the call.
    let result = unsafe {
        let call: unsafe extern "C" fn(*mut u64) -> i64 = std::mem::transmute(module.get_finalized_function(entry));
        call(&mut input)
    };
    assert_eq!(result, -18, "-(12 + 5 + (5 < 7)) after the stack round trip");
    assert_eq!(input, 18, "trusted store publishes the computed value");
}
