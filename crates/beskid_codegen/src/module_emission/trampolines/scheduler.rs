use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, types};
use cranelift_codegen::isa::{CallConv, TargetIsa};
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::import_helpers::{import_local_with_call_conv, import_with_call_conv};
use super::types::SchedulerCompletionTransfer;
use crate::module_emission::contracts::{SyntaxModuleEmissionError, emission_verification};

pub(in crate::module_emission) fn emit_scheduler_fiber_entry(
    isa: &dyn TargetIsa,
    scheduler_current_symbol: &str,
    fiber_done_symbol: &str,
    completion_transfer: &SchedulerCompletionTransfer<'_>,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let tail_completion = matches!(completion_transfer, SchedulerCompletionTransfer::Tail { .. });
    let mut signature = Signature::new(if tail_completion { CallConv::Tail } else { isa.default_call_conv() });
    signature.params.push(AbiParam::new(pointer));
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        let fiber = builder.block_params(block)[0];
        let entry = builder.ins().load(pointer, cranelift_codegen::ir::MemFlagsData::trusted(), fiber, 8);
        let argument = builder.ins().load(pointer, cranelift_codegen::ir::MemFlagsData::trusted(), fiber, 16);
        let mut body_signature = Signature::new(isa.default_call_conv());
        body_signature.params.push(AbiParam::new(pointer));
        body_signature.returns.push(AbiParam::new(types::I64));
        let body_signature = builder.import_signature(body_signature);
        let body_call = builder.ins().call_indirect(body_signature, entry, &[argument]);
        let result = builder.inst_results(body_call)[0];
        let current = import_local_with_call_conv(
            &mut builder,
            scheduler_current_symbol,
            &[],
            Some(pointer),
            isa.default_call_conv(),
        );
        let current_call = builder.ins().call(current, &[]);
        let index = builder.inst_results(current_call)[0];
        let fiber_done = import_local_with_call_conv(
            &mut builder,
            fiber_done_symbol,
            &[pointer, types::I64],
            None,
            isa.default_call_conv(),
        );
        builder.ins().call(fiber_done, &[index, result]);
        if tail_completion {
            let completion = import_with_call_conv(
                &mut builder,
                "__beskid_scheduler_return_trampoline",
                &[],
                None,
                CallConv::Tail,
                false,
            );
            builder.ins().return_call(completion, &[]);
        } else {
            // Context initialization installs the scheduler return trampoline as the entry's
            // return address on targets without the x86-64 System V tail-transfer path.
            builder.ins().return_(&[]);
        }
        builder.finalize(isa.frontend_config());
    }
    verify_function(&function, isa.flags())
        .map_err(|error| emission_verification(format!("scheduler fiber entry verification failed: {error}")))?;
    Ok(function)
}

pub(in crate::module_emission) fn emit_scheduler_return_trampoline(
    isa: &dyn TargetIsa,
    scheduler_current_symbol: &str,
    fiber_record_symbol: &str,
    scheduler_context_symbol: &str,
    scheduler_set_current_symbol: &str,
    context_switch_symbol: &str,
    completion_transfer: &SchedulerCompletionTransfer<'_>,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let tail_completion = matches!(completion_transfer, SchedulerCompletionTransfer::Tail { .. });
    let signature = Signature::new(if tail_completion { CallConv::Tail } else { isa.default_call_conv() });
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let current = import_local_with_call_conv(
            &mut builder,
            scheduler_current_symbol,
            &[],
            Some(pointer),
            isa.default_call_conv(),
        );
        let current_call = builder.ins().call(current, &[]);
        let index = builder.inst_results(current_call)[0];
        let record = import_local_with_call_conv(
            &mut builder,
            fiber_record_symbol,
            &[pointer],
            Some(pointer),
            isa.default_call_conv(),
        );
        let record_call = builder.ins().call(record, &[index]);
        let fiber = builder.inst_results(record_call)[0];
        let none = builder.ins().iconst(pointer, 0xFFFF);
        let set_current = import_local_with_call_conv(
            &mut builder,
            scheduler_set_current_symbol,
            &[pointer],
            None,
            isa.default_call_conv(),
        );
        builder.ins().call(set_current, &[none]);
        let scheduler_context = import_local_with_call_conv(
            &mut builder,
            scheduler_context_symbol,
            &[],
            Some(pointer),
            isa.default_call_conv(),
        );
        let scheduler_call = builder.ins().call(scheduler_context, &[]);
        let scheduler = builder.inst_results(scheduler_call)[0];
        let fiber_context = builder.ins().load(pointer, cranelift_codegen::ir::MemFlagsData::trusted(), fiber, 104);
        if let SchedulerCompletionTransfer::Tail { context_switch_symbol } = completion_transfer {
            let switch = import_with_call_conv(
                &mut builder,
                context_switch_symbol,
                &[pointer, pointer],
                None,
                isa.default_call_conv(),
                false,
            );
            let switch_address = builder.ins().func_addr(pointer, switch);
            let mut tail_signature = Signature::new(CallConv::Tail);
            tail_signature.params.extend([pointer, pointer].into_iter().map(AbiParam::new));
            let tail_signature = builder.import_signature(tail_signature);
            builder.ins().return_call_indirect(tail_signature, switch_address, &[fiber_context, scheduler]);
        } else {
            let switch = import_local_with_call_conv(
                &mut builder,
                context_switch_symbol,
                &[pointer, pointer],
                None,
                isa.default_call_conv(),
            );
            builder.ins().call(switch, &[fiber_context, scheduler]);
            builder.ins().return_(&[]);
        }
        builder.finalize(isa.frontend_config());
    }
    verify_function(&function, isa.flags())
        .map_err(|error| emission_verification(format!("scheduler return trampoline verification failed: {error}")))?;
    Ok(function)
}
