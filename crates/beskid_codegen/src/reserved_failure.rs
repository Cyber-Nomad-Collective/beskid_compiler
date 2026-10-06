//! Immutable specialized failure admission from query-issued constructor identity.
use crate::{AggregateStaticPlan, CodegenInput};
use beskid_queries::{
    AstNodeKey, GenericSpecializationInstance, SemanticTypeId, enum_constructor, enum_constructor_specialization,
    enum_layout, reserved_failure_constructor,
};

#[derive(Debug, Clone)]
pub struct ReservedFailureStaticPlan {
    result: AggregateStaticPlan,
    failure: AggregateStaticPlan,
    result_tag: u32,
    failure_tag: u32,
    result_tag_offset: u64,
    failure_tag_offset: u64,
    payload_offset: u64,
}
impl ReservedFailureStaticPlan {
    pub fn result(&self) -> &AggregateStaticPlan {
        &self.result
    }
    pub fn failure(&self) -> &AggregateStaticPlan {
        &self.failure
    }
}

/// Native-only invocation entry. Its two trailing words are an admitted immutable failure
/// handle and an already registered caller destination address, never source-visible values.
pub struct EmittedCheckedInvocation {
    function: cranelift_module::FuncId,
    source_parameter_count: usize,
}
impl EmittedCheckedInvocation {
    pub fn function(&self) -> cranelift_module::FuncId {
        self.function
    }
    pub fn source_parameter_count(&self) -> usize {
        self.source_parameter_count
    }
}

/// Emit an admitted checked entry in the producer's existing module. The caller supplies
/// its artifact-owned literal interner and canonical imported runtime IDs; descriptor and
/// literal publication remain the ordinary module data pass. No effect proof or function ID
/// supplied by a loader is accepted as authority.
pub fn emit_admitted_checked_entry<'db, M: cranelift_module::Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn cranelift_codegen::isa::TargetIsa,
    entry: AstNodeKey,
    specialization: Option<&GenericSpecializationInstance>,
    literal_context: &mut crate::CodegenContext,
    runtime_functions: &std::collections::HashMap<String, cranelift_module::FuncId>,
    namespace: &str,
    invocation_symbol: &str,
) -> cranelift_module::ModuleResult<EmittedCheckedInvocation> {
    let error = |message: &str| cranelift_module::ModuleError::Backend(anyhow::anyhow!("{message}"));
    // Reject mismatched applied entries before emitting any private callable.
    if specialization.is_some_and(|instance| instance.declaration != entry) {
        return Err(error("checked entry specialization declaration mismatch"));
    }
    input
        .checked_failure_destination_plan(entry, specialization)
        .ok_or_else(|| error("checked entry lacks source-issued failure reservation"))?;
    // Declare the canonical artifact pool's source literal symbols before CLIF remapping;
    // the producer still defines their bytes once through emit_string_literals afterward.
    use beskid_isle::NodeFacts;
    let effect = input
        .checked_effect_closure(isa, entry, specialization.cloned())
        .map_err(|failure| error(&format!("checked entry effect unavailable: {failure:?}")))?;
    for (_, member) in effect.members() {
        let facts = match member.specialization() {
            Some(instance) => crate::isle_adapter::SyntaxNodeFacts::new_with_item_specialization(
                input,
                isa,
                member.key(),
                instance.clone(),
            ),
            None => crate::isle_adapter::SyntaxNodeFacts::new_with_isa(input, isa),
        };
        let mut pending =
            beskid_queries::item_body(input.database(), member.key()).ok().flatten().into_iter().collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            if let Some(text) = facts.string_literal(node) {
                let symbol = literal_context.intern_string_literal(text.as_bytes());
                module.declare_data(&symbol, cranelift_module::Linkage::Local, false, false)?;
            }
            pending.extend(
                beskid_queries::child_nodes(input.database(), node)
                    .map_err(|failure| error(&format!("checked literal source unavailable: {failure:?}")))?
                    .unwrap_or_default()
                    .iter()
                    .copied(),
            );
        }
    }
    let mut string_interner = crate::module_emission::imports::ArtifactStringInterner {
        context: literal_context,
        pointer_type: module.target_config().pointer_type(),
    };
    let closure = if beskid_queries::canonical_serialization_encode(input.database(), entry) {
        let instance =
            specialization.ok_or_else(|| error("canonical Encode requires its current applied specialization"))?;
        crate::checked_effect::emit_serialization_checked_closure(
            module,
            input,
            isa,
            instance.clone(),
            &mut string_interner,
            runtime_functions,
            namespace,
        )?
    } else {
        let proof = input
            .checked_effect_closure(isa, entry, specialization.cloned())
            .map_err(|failure| error(&format!("checked entry effect unavailable: {failure:?}")))?;
        crate::checked_effect::emit_checked_effect_closure(
            module,
            input,
            isa,
            &proof,
            &mut string_interner,
            runtime_functions,
            namespace,
        )?
    };
    emit_checked_invocation(module, input, isa, entry, specialization, &closure, invocation_symbol)
}

/// Publish a complete typed result only through the caller's existing root lease. This entry
/// does not admit a root after computation, and cannot turn a foreign handle into a typed error.
/// The loader/compiler caller must acquire the destination lease before argument evaluation.
pub fn emit_checked_invocation<M: cranelift_module::Module>(
    module: &mut M,
    input: &CodegenInput<'_>,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    entry: AstNodeKey,
    specialization: Option<&GenericSpecializationInstance>,
    closure: &crate::checked_effect::EmittedCheckedClosure,
    symbol: &str,
) -> cranelift_module::ModuleResult<EmittedCheckedInvocation> {
    use cranelift_codegen::ir::condcodes::IntCC;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, types};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_module::{Linkage, ModuleError};
    let error = |message: &str| ModuleError::Backend(anyhow::anyhow!("{message}"));
    let plan = input
        .checked_failure_destination_plan(entry, specialization)
        .ok_or_else(|| error("checked invocation lacks exact failure destination"))?;
    let effect = input
        .checked_effect_closure(isa, entry, specialization.cloned())
        .map_err(|_| error("checked invocation effect is unavailable"))?;
    if closure.member(effect.entry()) != Some(closure.entry()) {
        return Err(error("checked invocation closure entry mismatch"));
    }
    let pointer = module.target_config().pointer_type();
    let scope = input
        .abi_manifest()
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidCheckedAllocationScope")
        .ok_or_else(|| error("checked invocation scope geometry unavailable"))?;
    if pointer != types::I64 || scope.size != 64 || scope.alignment != 8 {
        return Err(error("checked invocation scope geometry mismatch"));
    }
    let source_signature = module.declarations().get_function_decl(closure.entry()).signature.clone();
    if source_signature.returns.len() != 1 || source_signature.returns[0].value_type != pointer {
        return Err(error("checked invocation requires exact managed Result return ABI"));
    }
    let rollback = if beskid_queries::canonical_serialization_encode(input.database(), entry) {
        let instance = specialization.ok_or_else(|| error("Encode requires its exact applied specialization"))?;
        input
            .serialization_publication_plan(isa, instance.clone())
            .map_err(|_| error("Encoder publication/rollback proof unavailable"))?;
        let id = closure.rollback().ok_or_else(|| error("Encode closure lacks nonallocating publication clones"))?;
        let signature = &module.declarations().get_function_decl(id).signature;
        if source_signature.params.len() != 3
            || signature.params.len() != 1
            || signature.params[0] != source_signature.params[1]
            || !signature.returns.is_empty()
        {
            return Err(error("Encoder rollback receiver ABI mismatch"));
        }
        Some(id)
    } else {
        if closure.rollback().is_some() {
            return Err(error("foreign entry carries Encoder rollback capability"));
        }
        None
    };
    let result_descriptor = match module.get_name(&plan.result.descriptor_symbol) {
        Some(cranelift_module::FuncOrDataId::Data(id)) => id,
        _ => return Err(error("checked result descriptor not emitted")),
    };
    let failure_descriptor = match module.get_name(&plan.failure.descriptor_symbol) {
        Some(cranelift_module::FuncOrDataId::Data(id)) => id,
        _ => return Err(error("checked failure descriptor not emitted")),
    };
    let mut context = module.make_context();
    context.func.signature = source_signature.clone();
    let source_parameter_count = context.func.signature.params.len();
    context.func.signature.params.extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
    let function = module.declare_function(symbol, Linkage::Local, &context.func.signature)?;
    let mut import =
        |name: &str, parameters: &[cranelift_codegen::ir::Type], result: Option<cranelift_codegen::ir::Type>| {
            let mut signature = module.make_signature();
            signature.params.extend(parameters.iter().copied().map(AbiParam::new));
            signature.returns.extend(result.map(AbiParam::new));
            module.declare_function(name, Linkage::Import, &signature)
        };
    let resolve = import("gc_resolve_handle", &[pointer], Some(pointer))?;
    let admitted = import("beskid_rt_v5_gc_root_slot_is_registered", &[pointer], Some(types::I8))?;
    let enter = import("beskid_rt_v5_checked_scope_enter", &[pointer, pointer], Some(types::I8))?;
    let status = import("beskid_rt_v5_checked_scope_failure_reason", &[], Some(pointer))?;
    let leave = import("beskid_rt_v5_checked_scope_leave", &[pointer], None)?;
    let resolve = module.declare_func_in_func(resolve, &mut context.func);
    let admitted = module.declare_func_in_func(admitted, &mut context.func);
    let enter = module.declare_func_in_func(enter, &mut context.func);
    let status = module.declare_func_in_func(status, &mut context.func);
    let leave = module.declare_func_in_func(leave, &mut context.func);
    let rollback = rollback.map(|id| module.declare_func_in_func(id, &mut context.func));
    let body = module.declare_func_in_func(closure.entry(), &mut context.func);
    let result_descriptor = module.declare_data_in_func(result_descriptor, &mut context.func);
    let failure_descriptor = module.declare_data_in_func(failure_descriptor, &mut context.func);
    let header = input
        .abi_manifest()
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidObjectHeader")
        .ok_or_else(|| error("checked object header unavailable"))?;
    let descriptor_offset = header
        .fields
        .iter()
        .find(|field| field.name == "descriptor")
        .and_then(|field| i32::try_from(field.offset).ok())
        .ok_or_else(|| error("checked descriptor offset unavailable"))?;
    let tag_offset =
        i32::try_from(plan.result_tag_offset).map_err(|_| error("checked Result tag offset unavailable"))?;
    let payload_offset =
        i32::try_from(plan.payload_offset).map_err(|_| error("checked Result payload offset unavailable"))?;
    let failure_tag_offset =
        i32::try_from(plan.failure_tag_offset).map_err(|_| error("checked error tag offset unavailable"))?;
    let mut frontend = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        let parameters = builder.block_params(block).to_vec();
        let handle = parameters[source_parameter_count];
        let destination = parameters[source_parameter_count + 1];
        let call = builder.ins().call(admitted, &[destination]);
        let is_admitted = builder.inst_results(call)[0];
        builder.ins().trapz(is_admitted, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let call = builder.ins().call(resolve, &[handle]);
        let failure = builder.inst_results(call)[0];
        builder.ins().trapz(failure, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let descriptor = builder.ins().load(pointer, MemFlagsData::new(), failure, descriptor_offset);
        let expected = builder.ins().symbol_value(pointer, result_descriptor);
        let correct = builder.ins().icmp(IntCC::Equal, descriptor, expected);
        builder.ins().trapz(correct, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let tag = builder.ins().load(types::I32, MemFlagsData::new(), failure, tag_offset);
        let correct = builder.ins().icmp_imm_u(IntCC::Equal, tag, i64::from(plan.result_tag));
        builder.ins().trapz(correct, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let error_value = builder.ins().load(pointer, MemFlagsData::new(), failure, payload_offset);
        builder.ins().trapz(error_value, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let descriptor = builder.ins().load(pointer, MemFlagsData::new(), error_value, descriptor_offset);
        let expected = builder.ins().symbol_value(pointer, failure_descriptor);
        let correct = builder.ins().icmp(IntCC::Equal, descriptor, expected);
        builder.ins().trapz(correct, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let tag = builder.ins().load(types::I32, MemFlagsData::new(), error_value, failure_tag_offset);
        let correct = builder.ins().icmp_imm_u(IntCC::Equal, tag, i64::from(plan.failure_tag));
        builder.ins().trapz(correct, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let storage = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 64, 3));
        let zero = builder.ins().iconst(pointer, 0);
        for offset in (0..64).step_by(8) {
            builder.ins().stack_store(pointer, zero, storage, offset);
        }
        let address = builder.ins().stack_addr(pointer, storage, 0);
        let call = builder.ins().call(enter, &[address, handle]);
        let entered = builder.inst_results(call)[0];
        builder.ins().trapz(entered, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        let call = builder.ins().call(body, &parameters[..source_parameter_count]);
        let result = builder.inst_results(call)[0];
        let call = builder.ins().call(status, &[]);
        let reason = builder.inst_results(call)[0];
        let failed = builder.ins().icmp_imm_u(IntCC::NotEqual, reason, 0);
        if let Some(rollback) = rollback {
            let cleanup = builder.create_block();
            let resume = builder.create_block();
            builder.ins().brif(failed, cleanup, &[], resume, &[]);
            builder.switch_to_block(cleanup);
            builder.seal_block(cleanup);
            // Source-issued allocation-free rollback executes even while failure is sticky.
            // The caller roots the receiver throughout and this clone cannot allocate/GC.
            builder.ins().call(rollback, &[parameters[1]]);
            builder.ins().jump(resume, &[]);
            builder.switch_to_block(resume);
            builder.seal_block(resume);
        }
        let published = builder.ins().select(failed, failure, result);
        builder.ins().trapz(published, cranelift_codegen::ir::TrapCode::unwrap_user(10));
        // Leave restores the prior invocation before caller-visible stores; both candidates
        // remain protected without a managed safepoint before this preadmitted root store.
        builder.ins().call(leave, &[address]);
        builder.ins().store(MemFlagsData::new(), published, destination, 0);
        builder.ins().return_(&[published]);
    }
    module.define_function(function, &mut context)?;
    Ok(EmittedCheckedInvocation { function, source_parameter_count })
}

impl CodegenInput<'_> {
    /// Resolve a callable's concrete Result success identity through registered source facts.
    /// An erased pointer return signature alone cannot authorize immutable failure admission.
    pub fn checked_failure_destination_plan(
        &self,
        entry: AstNodeKey,
        specialization: Option<&GenericSpecializationInstance>,
    ) -> Option<ReservedFailureStaticPlan> {
        if specialization.is_some_and(|value| value.declaration != entry) {
            return None;
        }
        let substitutions = specialization.map(|value| value.substitutions.clone()).unwrap_or_default();
        let destination =
            beskid_queries::checked_failure_destination(self.database(), entry, substitutions).ok().flatten()?;
        self.reserved_failure_static_plan(destination.constructor().result(), Some(destination.factory()))
    }

    pub fn reserved_failure_static_plan(
        &self,
        key: AstNodeKey,
        specialization: Option<&GenericSpecializationInstance>,
    ) -> Option<ReservedFailureStaticPlan> {
        let substitutions = specialization.map(|value| value.substitutions.clone()).unwrap_or_default();
        let authority = reserved_failure_constructor(self.database(), key, substitutions.clone()).ok().flatten()?;
        if specialization.is_some_and(|value| value.declaration != authority.factory()) {
            return None;
        }
        let specialized =
            enum_constructor_specialization(self.database(), authority.result(), substitutions).ok().flatten();
        let (constructor, result_layout) = if let Some(fact) = specialized {
            (fact.constructor, fact.layout)
        } else {
            (
                enum_constructor(self.database(), authority.result()).ok().flatten()?,
                enum_layout(self.database(), authority.result()).ok().flatten()?,
            )
        };
        let failure_constructor = enum_constructor(self.database(), authority.failure()).ok().flatten()?;
        let failure_layout = enum_layout(self.database(), authority.failure()).ok().flatten()?;
        let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let result_physical =
            result_layout.scalar_payload_object_layout(self.target().pointer_width, header.size, header.alignment)?;
        let failure_physical =
            failure_layout.scalar_payload_object_layout(self.target().pointer_width, header.size, header.alignment)?;
        let [Some((SemanticTypeId::POINTER, payload_offset))] =
            result_physical.variants.get(constructor.variant_index as usize)?.payload_fields.as_ref()
        else {
            return None;
        };
        failure_physical
            .variants
            .get(failure_constructor.variant_index as usize)?
            .payload_fields
            .is_empty()
            .then_some(())?;
        result_physical.pointer_map_offsets.contains(payload_offset).then_some(())?;
        Some(ReservedFailureStaticPlan {
            result: self.enum_static_plan_for_specialization(authority.result(), specialization)?,
            failure: self.enum_static_plan(authority.failure())?,
            result_tag: constructor.variant_index,
            failure_tag: failure_constructor.variant_index,
            result_tag_offset: result_physical.tag_offset,
            failure_tag_offset: failure_physical.tag_offset,
            payload_offset: *payload_offset,
        })
    }
}

/// Emit admission after the single aggregate descriptor pass. The returned canonical handle
/// belongs to the current process/heap domain; absence forbids callable publication.
pub fn emit_reserved_failure_admission<M: cranelift_module::Module>(
    module: &mut M,
    plan: &ReservedFailureStaticPlan,
    result_request: cranelift_module::DataId,
    failure_request: cranelift_module::DataId,
    symbol: &str,
) -> cranelift_module::ModuleResult<cranelift_module::FuncId> {
    use cranelift_codegen::ir::condcodes::IntCC;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, types};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_module::{Linkage, ModuleError};
    let error = |message: &str| ModuleError::Backend(anyhow::anyhow!("{message}"));
    if module.declarations().get_data_decl(result_request).name.as_deref()
        != Some(plan.result.allocation_request_symbol.as_str())
        || module.declarations().get_data_decl(failure_request).name.as_deref()
            != Some(plan.failure.allocation_request_symbol.as_str())
    {
        return Err(error("reserved failure admission request identity mismatch"));
    }
    let pointer = module.target_config().pointer_type();
    let mut context = module.make_context();
    context.func.signature.returns.push(AbiParam::new(pointer));
    let function = module.declare_function(symbol, Linkage::Local, &context.func.signature)?;
    let mut allocate_signature = module.make_signature();
    allocate_signature.params.push(AbiParam::new(pointer));
    allocate_signature.returns.push(AbiParam::new(pointer));
    let allocate =
        module.declare_function("beskid_rt_v5_managed_object_try_allocate", Linkage::Import, &allocate_signature)?;
    let root = module.declare_function("beskid_rt_v5_gc_try_root_handle", Linkage::Import, &allocate_signature)?;
    let mut unroot_signature = module.make_signature();
    unroot_signature.params.push(AbiParam::new(pointer));
    let unroot = module.declare_function("gc_unroot_handle", Linkage::Import, &unroot_signature)?;
    let allocate = module.declare_func_in_func(allocate, &mut context.func);
    let root = module.declare_func_in_func(root, &mut context.func);
    let unroot = module.declare_func_in_func(unroot, &mut context.func);
    let failure_request = module.declare_data_in_func(failure_request, &mut context.func);
    let result_request = module.declare_data_in_func(result_request, &mut context.func);
    let mut frontend = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
        let entry = builder.create_block();
        let failure_rooted = builder.create_block();
        let failure_ready = builder.create_block();
        let result_ready = builder.create_block();
        let success = builder.create_block();
        let failed = builder.create_block();
        let keeper = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let zero = builder.ins().iconst(pointer, 0);
        builder.ins().stack_store(pointer, zero, keeper, 0);
        let request = builder.ins().symbol_value(pointer, failure_request);
        let call = builder.ins().call(allocate, &[request]);
        let failure = builder.inst_results(call)[0];
        let present = builder.ins().icmp_imm_u(IntCC::NotEqual, failure, 0);
        builder.ins().brif(present, failure_ready, &[], failed, &[]);
        builder.switch_to_block(failure_ready);
        builder.seal_block(failure_ready);
        let tag = builder.ins().iconst(types::I32, i64::from(plan.failure_tag));
        builder.ins().store(
            MemFlagsData::new(),
            tag,
            failure,
            i32::try_from(plan.failure_tag_offset).map_err(|_| error("failure tag offset overflow"))?,
        );
        let call = builder.ins().call(root, &[failure]);
        let handle = builder.inst_results(call)[0];
        builder.ins().stack_store(pointer, handle, keeper, 0);
        let present = builder.ins().icmp_imm_u(IntCC::NotEqual, handle, 0);
        builder.ins().brif(present, failure_rooted, &[], failed, &[]);
        builder.switch_to_block(failure_rooted);
        builder.seal_block(failure_rooted);
        let request = builder.ins().symbol_value(pointer, result_request);
        let call = builder.ins().call(allocate, &[request]);
        let result = builder.inst_results(call)[0];
        let present = builder.ins().icmp_imm_u(IntCC::NotEqual, result, 0);
        builder.ins().brif(present, result_ready, &[], failed, &[]);
        builder.switch_to_block(result_ready);
        builder.seal_block(result_ready);
        let tag = builder.ins().iconst(types::I32, i64::from(plan.result_tag));
        builder.ins().store(
            MemFlagsData::new(),
            tag,
            result,
            i32::try_from(plan.result_tag_offset).map_err(|_| error("result tag offset overflow"))?,
        );
        builder.ins().store(
            MemFlagsData::new(),
            failure,
            result,
            i32::try_from(plan.payload_offset).map_err(|_| error("result payload offset overflow"))?,
        );
        let call = builder.ins().call(root, &[result]);
        let result_handle = builder.inst_results(call)[0];
        let present = builder.ins().icmp_imm_u(IntCC::NotEqual, result_handle, 0);
        builder.ins().brif(present, success, &[], failed, &[]);
        builder.switch_to_block(success);
        builder.seal_block(success);
        let keeper_handle = builder.ins().stack_load(pointer, pointer, keeper, 0);
        builder.ins().call(unroot, &[keeper_handle]);
        builder.ins().return_(&[result_handle]);
        builder.switch_to_block(failed);
        builder.seal_block(failed);
        let keeper_handle = builder.ins().stack_load(pointer, pointer, keeper, 0);
        builder.ins().call(unroot, &[keeper_handle]);
        builder.ins().return_(&[zero]);
        builder.finalize(module.target_config());
    }
    module.define_function(function, &mut context)?;
    Ok(function)
}
