use cranelift_codegen::ir::{AbiParam, ExternalName, Function, InstBuilder, Signature, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::import_helpers::import_local;
use super::types::SpawnTrampoline;
use crate::module_emission::contracts::{SyntaxModuleEmissionError, emission_verification};

const FIBER_ABI_CALL_RESERVE: u64 = 1 << 16;
const MAX_CRANELIFT_SPILL_BYTES_PER_VALUE: u64 = 16;
const FIBER_STACK_MAX_SIZE: u64 = 8 << 20;

/// Compute a pre-legalization upper bound for entering a generated fiber target.
///
/// Cranelift does not expose its final spill frame until compilation, after the point at which the
/// guard must be emitted. Reserve one complete initial stack increment for the trampoline and ABI
/// calls, reserve one maximum-width spill for every CLIF SSA value, then add every fixed stack slot
/// with its declared alignment. Dynamic slots have no finite pre-emission bound and are rejected
/// rather than relying on a guard fault.
pub(in crate::module_emission) fn conservative_fiber_stack_requirement(
    target: &Function,
    target_symbol: &str,
) -> Result<u64, SyntaxModuleEmissionError> {
    if !target.dynamic_stack_slots.is_empty() {
        return Err(emission_verification(format!(
            "fiber target `{target_symbol}` has an unbounded dynamic stack frame"
        )));
    }
    let value_count = u64::try_from(target.dfg.num_values()).map_err(|_| {
        emission_verification(format!("fiber target `{target_symbol}` value count is not representable"))
    })?;
    let spill_reserve = value_count
        .checked_mul(MAX_CRANELIFT_SPILL_BYTES_PER_VALUE)
        .ok_or_else(|| emission_verification(format!("fiber target `{target_symbol}` spill requirement overflowed")))?;
    let mut required = FIBER_ABI_CALL_RESERVE
        .checked_add(spill_reserve)
        .ok_or_else(|| emission_verification(format!("fiber target `{target_symbol}` stack requirement overflowed")))?;
    for slot in target.sized_stack_slots.values() {
        let alignment = 1u64.checked_shl(u32::from(slot.align_shift)).ok_or_else(|| {
            emission_verification(format!("fiber target `{target_symbol}` has invalid stack alignment"))
        })?;
        required = required
            .checked_add(alignment - 1)
            .map(|value| value & !(alignment - 1))
            .and_then(|value| value.checked_add(u64::from(slot.size)))
            .ok_or_else(|| {
                emission_verification(format!("fiber target `{target_symbol}` stack requirement overflowed"))
            })?;
    }
    if required > FIBER_STACK_MAX_SIZE {
        return Err(emission_verification(format!(
            "fiber target `{target_symbol}` requires {required} usable stack bytes, exceeding {FIBER_STACK_MAX_SIZE}"
        )));
    }
    Ok(required)
}

pub(in crate::module_emission) fn emit_spawn_trampoline(
    trampoline: &SpawnTrampoline,
    isa: &dyn TargetIsa,
    required_usable_size: u64,
    scheduler_stack_check_symbol: &str,
    scheduler_stack_overflow_symbol: &str,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let mut signature = Signature::new(isa.default_call_conv());
    signature.params.push(AbiParam::new(pointer));
    signature.returns.push(AbiParam::new(types::I64));
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let environment = builder.block_params(entry)[0];
        let stack_check = import_local(&mut builder, scheduler_stack_check_symbol, &[pointer], Some(types::I8));
        let required = builder.ins().iconst(
            pointer,
            i64::try_from(required_usable_size).map_err(|_| {
                emission_verification(format!(
                    "fiber target `{}` stack requirement is not representable",
                    trampoline.target_symbol
                ))
            })?,
        );
        let check_call = builder.ins().call(stack_check, &[required]);
        let allowed = builder.inst_results(check_call)[0];
        let body = builder.create_block();
        let overflow = builder.create_block();
        builder.ins().brif(allowed, body, &[], overflow, &[]);
        builder.seal_block(body);
        builder.seal_block(overflow);

        builder.switch_to_block(overflow);
        let observed = import_local(&mut builder, scheduler_stack_overflow_symbol, &[], None);
        builder.ins().call(observed, &[]);
        let overflow_result = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[overflow_result]);

        builder.switch_to_block(body);
        let target_signature = builder.import_signature(trampoline.target_signature.clone());
        let target = builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase(trampoline.target_symbol.as_bytes()),
            signature: target_signature,
            colocated: false,
            patchable: false,
        });
        let call = if trampoline.closure_captures.is_some() {
            builder.ins().call(target, &[environment])
        } else {
            builder.ins().call(target, &[])
        };
        let results = builder.inst_results(call).to_vec();
        let result = match results.as_slice() {
            [] => builder.ins().iconst(types::I64, 0),
            [value] => *value,
            _ => {
                return Err(emission_verification(format!(
                    "spawn trampoline target `{}` must return one typed ABI value",
                    trampoline.target_symbol
                )));
            }
        };
        let root = if !trampoline.result_plan.pointer_map_offsets.is_empty() {
            let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                pointer.bytes(),
                3,
            ));
            builder.ins().stack_store(pointer, result, slot, 0);
            let address = builder.ins().stack_addr(pointer, slot, 0);
            let register = import_local(&mut builder, "gc_register_root", &[pointer], Some(types::I8));
            let call = builder.ins().call(register, &[address]);
            let ok = builder.inst_results(call)[0];
            builder.ins().trapz(ok, cranelift_codegen::ir::TrapCode::unwrap_user(5));
            Some(address)
        } else {
            None
        };
        let request = builder.func.create_global_value(cranelift_codegen::ir::GlobalValueData::Symbol {
            name: ExternalName::testcase(&trampoline.result_plan.allocation_request_symbol),
            offset: 0.into(),
            colocated: false,
            tls: false,
        });
        let request = builder.ins().symbol_value(pointer, request);
        let allocate = import_local(&mut builder, "beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer));
        let call = builder.ins().call(allocate, &[request]);
        let boxed = builder.inst_results(call)[0];
        builder.ins().trapz(boxed, cranelift_codegen::ir::TrapCode::unwrap_user(5));
        builder.ins().store(
            cranelift_codegen::ir::MemFlagsData::new(),
            result,
            boxed,
            trampoline.result_plan.fields[0].field_offset as i32,
        );
        if let Some(root) = root {
            let unregister = import_local(&mut builder, "gc_unregister_root", &[pointer], None);
            builder.ins().call(unregister, &[root]);
        }
        builder.ins().return_(&[boxed]);
        builder.finalize(isa.frontend_config());
    }
    verify_function(&function, isa.flags()).map_err(|error| {
        emission_verification(format!("spawn trampoline `{}` verification failed: {error}", trampoline.symbol))
    })?;
    Ok(function)
}
