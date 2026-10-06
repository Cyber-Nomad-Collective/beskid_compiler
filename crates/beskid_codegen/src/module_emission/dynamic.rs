//! Source-issued Pack representation bridge, with ordinary source Result constructors.
use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::items::ResolvedSyntaxModuleItem;
use crate::CodegenInput;
use beskid_isle::DirectCallee;
use cranelift_codegen::{
    ir::{AbiParam, ExtFuncData, ExternalName, Function, GlobalValueData, InstBuilder, MemFlagsData, Signature},
    isa::TargetIsa,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use std::collections::HashMap;

/// Checked clone of the exact compiler-issued representation bridge. Factories
/// are the retained checked closure's source-qualified clone IDs, never raw callbacks.
pub(crate) fn emit_checked_pack_bridge<M: cranelift_module::Module>(
    module: &mut M,
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    pack: &beskid_queries::GenericSpecializationInstance,
    bridge: &beskid_queries::DynamicPackingBridge,
    valid_factory: cranelift_module::FuncId,
    invalid_factory: cranelift_module::FuncId,
    runtime_functions: &HashMap<String, cranelift_module::FuncId>,
) -> cranelift_module::ModuleResult<Function> {
    use cranelift_codegen::ir::condcodes::IntCC;
    use cranelift_module::{FuncOrDataId, ModuleError};
    let error = |message: &str| ModuleError::Backend(anyhow::anyhow!("{message}"));
    let current = beskid_queries::dynamic_packing_bridge(input.database(), pack)
        .map_err(|_| error("checked Pack bridge source proof unavailable"))?
        .ok_or_else(|| error("checked Pack bridge is not canonical"))?;
    if &current != bridge {
        return Err(error("checked Pack bridge source identity drift"));
    }
    let plan = input
        .aggregate_static_plan_for_specialization(bridge.box_literal(), Some(pack))
        .ok_or_else(|| error("checked Pack box descriptor unavailable"))?;
    let descriptor = match module.get_name(&plan.descriptor_symbol) {
        Some(FuncOrDataId::Data(id)) => id,
        _ => return Err(error("checked Pack descriptor was not emitted by the source pass")),
    };
    let pointer = isa.pointer_type();
    for (id, count) in [(valid_factory, 1), (invalid_factory, 0)] {
        let signature = &module.declarations().get_function_decl(id).signature;
        if signature.params.len() != count
            || signature.params.iter().any(|parameter| parameter.value_type != pointer)
            || signature.returns.len() != 1
            || signature.returns[0].value_type != pointer
        {
            return Err(error("checked Pack typed constructor ABI mismatch"));
        }
    }
    let provider = |name: &str| {
        runtime_functions
            .get(name)
            .copied()
            .filter(|id| module.declarations().get_function_decl(*id).name.as_deref() == Some(name))
            .ok_or_else(|| error("checked Pack canonical provider unavailable"))
    };
    let root = provider("gc_root_handle")?;
    let resolve = provider("gc_resolve_handle")?;
    let unroot = provider("gc_unroot_handle")?;
    let status = provider("beskid_rt_v5_checked_scope_failure_reason")?;
    let mut function = Function::new();
    function.signature = Signature::new(isa.default_call_conv());
    function.signature.params.push(AbiParam::new(pointer));
    function.signature.returns.push(AbiParam::new(pointer));
    let mut context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut context);
        let entry = builder.create_block();
        let rooted = builder.create_block();
        let identified = builder.create_block();
        let accepted = builder.create_block();
        let rejected = builder.create_block();
        let cleanup = builder.create_block();
        let failed = builder.create_block();
        builder.append_block_param(cleanup, pointer);
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let boxed = builder.block_params(entry)[0];
        let root = module.declare_func_in_func(root, builder.func);
        let call = builder.ins().call(root, &[boxed]);
        let keeper = builder.inst_results(call)[0];
        let status = module.declare_func_in_func(status, builder.func);
        let call = builder.ins().call(status, &[]);
        let reason = builder.inst_results(call)[0];
        let clean = builder.ins().icmp_imm_u(IntCC::Equal, reason, 0);
        let live = builder.ins().icmp_imm_u(IntCC::NotEqual, keeper, 0);
        let admitted = builder.ins().band(clean, live);
        builder.ins().brif(admitted, rooted, &[], failed, &[]);
        builder.switch_to_block(rooted);
        builder.seal_block(rooted);
        let resolve = module.declare_func_in_func(resolve, builder.func);
        let call = builder.ins().call(resolve, &[keeper]);
        let boxed = builder.inst_results(call)[0];
        let live = builder.ins().icmp_imm_u(IntCC::NotEqual, boxed, 0);
        builder.ins().brif(live, identified, &[], rejected, &[]);
        builder.switch_to_block(identified);
        builder.seal_block(identified);
        let actual = builder.ins().load(pointer, MemFlagsData::trusted(), boxed, 0);
        let descriptor = module.declare_data_in_func(descriptor, builder.func);
        let expected = builder.ins().symbol_value(pointer, descriptor);
        let same = builder.ins().icmp(IntCC::Equal, actual, expected);
        builder.ins().brif(same, accepted, &[], rejected, &[]);
        for (block, factory, arguments) in
            [(accepted, valid_factory, vec![boxed]), (rejected, invalid_factory, Vec::new())]
        {
            builder.switch_to_block(block);
            builder.seal_block(block);
            let factory = module.declare_func_in_func(factory, builder.func);
            let call = builder.ins().call(factory, &arguments);
            let result = builder.inst_results(call)[0];
            let call = builder.ins().call(status, &[]);
            let reason = builder.inst_results(call)[0];
            let clean = builder.ins().icmp_imm_u(IntCC::Equal, reason, 0);
            let zero = builder.ins().iconst(pointer, 0);
            let published = builder.ins().select(clean, result, zero);
            builder.ins().jump(cleanup, &[published.into()]);
        }
        builder.switch_to_block(cleanup);
        builder.seal_block(cleanup);
        let result = builder.block_params(cleanup)[0];
        let unroot = module.declare_func_in_func(unroot, builder.func);
        builder.ins().call(unroot, &[keeper]);
        builder.ins().return_(&[result]);
        builder.switch_to_block(failed);
        builder.seal_block(failed);
        // A root may exist even when the sticky failure was raised while admitting
        // it. Unrooting zero is a canonical no-op; this cleanup is allocation-free.
        builder.ins().call(unroot, &[keeper]);
        let zero = builder.ins().iconst(pointer, 0);
        builder.ins().return_(&[zero]);
        builder.finalize(isa.frontend_config());
    }
    Ok(function)
}

pub(super) fn emit_pack_bridge(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: &ResolvedSyntaxModuleItem,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
) -> Result<Option<Function>, SyntaxModuleEmissionError> {
    let Some(instance) = &item.specialization else { return Ok(None) };
    for pack in items.iter().filter_map(|item| item.specialization.as_ref()) {
        let Some(bridge) = beskid_queries::dynamic_packing_bridge(input.database(), pack)
            .map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        if bridge.instance() != instance {
            continue;
        }
        let plan = input
            .aggregate_static_plan_for_specialization(bridge.box_literal(), Some(pack))
            .ok_or_else(|| emission_verification("Pack box lacks its source-issued allocation descriptor"))?;
        if instance.signature.parameters.as_ref() != &[beskid_queries::SemanticTypeId::POINTER]
            || instance.signature.result != beskid_queries::SemanticTypeId::POINTER
        {
            return Err(emission_verification("Pack bridge ABI disagrees with its managed source identity"));
        }
        let factory = |key| {
            symbols
                .get(&DirectCallee::item(key))
                .cloned()
                .ok_or_else(|| emission_verification("Pack typed Result constructor was not emitted"))
        };
        let valid = factory(bridge.result_factory())?;
        let invalid = factory(bridge.invalid_factory())?;
        let pointer = isa.pointer_type();
        let mut signature = Signature::new(isa.default_call_conv());
        signature.params.push(AbiParam::new(pointer));
        signature.returns.push(AbiParam::new(pointer));
        let mut function = Function::new();
        function.signature = signature.clone();
        let mut context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut context);
            let entry = builder.create_block();
            let rooted = builder.create_block();
            let accepted = builder.create_block();
            let rejected = builder.create_block();
            let failed = builder.create_block();
            let identified = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let boxed = builder.block_params(entry)[0];
            let import = |builder: &mut FunctionBuilder<'_>, name: &str, params: usize| {
                let mut signature = Signature::new(isa.default_call_conv());
                signature.params.extend((0..params).map(|_| AbiParam::new(pointer)));
                signature.returns.push(AbiParam::new(pointer));
                let signature = builder.import_signature(signature);
                builder.func.import_function(ExtFuncData {
                    name: ExternalName::testcase(name),
                    signature,
                    colocated: false,
                    patchable: false,
                })
            };
            let root = import(&mut builder, "gc_root_handle", 1);
            let call = builder.ins().call(root, &[boxed]);
            let keeper = builder.inst_results(call)[0];
            let live = builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::NotEqual, keeper, 0);
            builder.ins().brif(live, rooted, &[], failed, &[]);
            builder.switch_to_block(rooted);
            builder.seal_block(rooted);
            let resolve = import(&mut builder, "gc_resolve_handle", 1);
            let call = builder.ins().call(resolve, &[keeper]);
            let boxed = builder.inst_results(call)[0];
            let resolved = builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::NotEqual, boxed, 0);
            builder.ins().brif(resolved, identified, &[], rejected, &[]);
            builder.switch_to_block(identified);
            builder.seal_block(identified);
            // Root admission already validated current managed allocation membership.
            let descriptor = builder.ins().load(pointer, MemFlagsData::trusted(), boxed, 0);
            let global = builder.func.create_global_value(GlobalValueData::Symbol {
                name: ExternalName::testcase(&plan.descriptor_symbol),
                offset: 0.into(),
                colocated: false,
                tls: false,
            });
            let expected = builder.ins().symbol_value(pointer, global);
            let same = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, descriptor, expected);
            builder.ins().brif(same, accepted, &[], rejected, &[]);
            for (block, symbol, arguments) in [(accepted, valid, vec![boxed]), (rejected, invalid, Vec::new())] {
                builder.switch_to_block(block);
                builder.seal_block(block);
                let factory = import(&mut builder, &symbol, arguments.len());
                let call = builder.ins().call(factory, &arguments);
                let result = builder.inst_results(call)[0];
                let mut unroot = Signature::new(isa.default_call_conv());
                unroot.params.push(AbiParam::new(pointer));
                let unroot = builder.import_signature(unroot);
                let unroot = builder.func.import_function(ExtFuncData {
                    name: ExternalName::testcase("gc_unroot_handle"),
                    signature: unroot,
                    colocated: false,
                    patchable: false,
                });
                builder.ins().call(unroot, &[keeper]);
                builder.ins().return_(&[result]);
            }
            builder.switch_to_block(failed);
            builder.seal_block(failed);
            let zero = builder.ins().iconst(pointer, 0);
            builder.ins().return_(&[zero]);
            builder.finalize(isa.frontend_config());
        }
        return Ok(Some(function));
    }
    Ok(None)
}

/// Shape's bodyless method is admitted only through the exact canonical source
/// call and type graph. Its callee is emitted by the sealed native initializer.
pub(super) fn emit_shape_bridge(
    input:&CodegenInput<'_>,isa:&dyn TargetIsa,item:&ResolvedSyntaxModuleItem,
    items:&[ResolvedSyntaxModuleItem],
)->Result<Option<Function>,SyntaxModuleEmissionError> {
    let Some(instance)=item.specialization.as_ref() else{return Ok(None)};
    for requested in items.iter().filter_map(|item|item.specialization.as_ref()) {
        let Some(binding)=beskid_queries::dynamic_shape_binding(input.database(),requested)
            .map_err(|error|emission_verification(error.to_string()))? else{continue};
        if binding.instance()!=instance{continue}
        if !instance.signature.parameters.is_empty() || instance.signature.result!=beskid_queries::SemanticTypeId::U64 {
            return Err(emission_verification("canonical Shape bridge ABI mismatch"));
        }
        let shape=input.compiled_dynamic_packing_shape(binding.packing())
            .map_err(|error|emission_verification(error))?.ok_or_else(||emission_verification("canonical Shape graph unavailable"))?;
        let symbol=format!("beskid_dynamic_shape_tag_v1_{}",shape.sha256().iter().map(|byte|format!("{byte:02x}")).collect::<String>());
        let mut function=Function::new();function.signature=Signature::new(isa.default_call_conv());
        function.signature.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
        let mut context=FunctionBuilderContext::new();
        {
            let mut builder=FunctionBuilder::new(&mut function,&mut context);let entry=builder.create_block();
            builder.switch_to_block(entry);builder.seal_block(entry);
            let called_signature=builder.func.signature.clone();
            let signature=builder.import_signature(called_signature);
            let callee=builder.func.import_function(ExtFuncData{name:ExternalName::testcase(&symbol),signature,colocated:false,patchable:false});
            let call=builder.ins().call(callee,&[]);let tag=builder.inst_results(call)[0];
            let absent=builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::Equal,tag,0);
            builder.ins().trapnz(absent,cranelift_codegen::ir::TrapCode::unwrap_user(1));
            builder.ins().return_(&[tag]);builder.finalize(isa.frontend_config());
        }
        return Ok(Some(function));
    }
    Ok(None)
}

pub(super) fn emit_unpack_bridge(
    input:&CodegenInput<'_>,isa:&dyn TargetIsa,item:&ResolvedSyntaxModuleItem,
    items:&[ResolvedSyntaxModuleItem],symbols:&HashMap<DirectCallee,String>,
)->Result<Option<Function>,SyntaxModuleEmissionError> {
    let Some(instance)=item.specialization.as_ref() else{return Ok(None)};
    for requested in items.iter().filter_map(|item|item.specialization.as_ref()) {
        let Some(bridge)=beskid_queries::dynamic_unpacking_bridge(input.database(),requested)
            .map_err(|e|emission_verification(e.to_string()))? else{continue};
        if bridge.instance()!=instance{continue}
        let symbol=|factory:&beskid_queries::GenericSpecializationInstance| {
            let identity=beskid_queries::generic_specialization_identity(factory);
            symbols.get(&DirectCallee::specialized_item(factory.declaration,identity)).cloned()
                .ok_or_else(||emission_verification("typed Unpack constructor was not emitted"))
        };
        return build_unpack_function(input,isa,&bridge,&symbol(bridge.result_factory())?,
            &symbol(bridge.invalid_factory())?,false).map(Some);
    }
    Ok(None)
}

/// Checked extraction uses only retained source-issued bridge/factory proofs.
/// Factory IDs are the caller's checked clones, not generator-supplied callbacks.
pub(crate) fn emit_checked_unpack_bridge<M:cranelift_module::Module>(
    module:&mut M,input:&CodegenInput<'_>,isa:&dyn TargetIsa,
    unpack:&beskid_queries::GenericSpecializationInstance,bridge:&beskid_queries::DynamicUnpackingBridge,
    valid_factory:cranelift_module::FuncId,invalid_factory:cranelift_module::FuncId,
    runtime_functions:&HashMap<String,cranelift_module::FuncId>,
)->cranelift_module::ModuleResult<Function> {
    let error=|message:String|cranelift_module::ModuleError::Backend(anyhow::anyhow!("{message}"));
    let reissued=beskid_queries::dynamic_unpacking_bridge(input.database(),unpack)
        .map_err(|e|error(e.to_string()))?.ok_or_else(||error("checked Unpack source proof unavailable".into()))?;
    if &reissued!=bridge{return Err(error("checked Unpack source proof changed".into()))}
    let names=["__beskid_unpack_valid","__beskid_unpack_invalid"];
    let mut imports=runtime_functions.clone();
    for (name,id,instance) in [(names[0],valid_factory,bridge.result_factory()),(names[1],invalid_factory,bridge.invalid_factory())] {
        let mut expected=Signature::new(isa.default_call_conv());
        for ty in instance.signature.parameters.iter().copied().filter(|ty|*ty!=beskid_queries::SemanticTypeId::UNIT) {
            expected.params.push(AbiParam::new(crate::isle_adapter::mappings::map_signature_type(isa,ty)
                .ok_or_else(||error("checked Unpack factory parameter ABI unavailable".into()))?));
        }
        expected.returns.push(AbiParam::new(crate::isle_adapter::mappings::map_signature_type(isa,instance.signature.result)
            .ok_or_else(||error("checked Unpack factory result ABI unavailable".into()))?));
        if module.declarations().get_function_decl(id).signature!=expected {
            return Err(error("checked Unpack factory ABI mismatch".into()))
        }
        imports.insert(name.into(),id);
    }
    let function=build_unpack_function(input,isa,bridge,names[0],names[1],true)
        .map_err(|e|error(e.to_string()))?;
    let mut context=module.make_context();context.func=function;
    crate::cranelift_host::remap_testcase_externals(module,&mut context,&imports)
        .map_err(|e|error(e.to_string()))?;
    Ok(context.func)
}

fn build_unpack_function(
    input:&CodegenInput<'_>,isa:&dyn TargetIsa,bridge:&beskid_queries::DynamicUnpackingBridge,
    valid_symbol:&str,invalid_symbol:&str,checked:bool,
)->Result<Function,SyntaxModuleEmissionError> {
    use cranelift_codegen::ir::condcodes::IntCC;
    let pack=bridge.packing();
    let packing=beskid_queries::dynamic_packing_bridge(input.database(),pack)
        .map_err(|e|emission_verification(e.to_string()))?
        .ok_or_else(||emission_verification("Unpack has no canonical box declaration"))?;
    let plan=input.aggregate_static_plan_for_specialization(packing.box_literal(),Some(pack))
        .ok_or_else(||emission_verification("Unpack source box layout unavailable"))?;
    if plan.fields.len()!=1{return Err(emission_verification("canonical Pack box field count changed"))}
    let field=plan.fields[0];let pointer=isa.pointer_type();
    let value_type=if field.abi_type==beskid_queries::SemanticTypeId::UNIT{None}else{
        Some(crate::isle_adapter::mappings::map_signature_type(isa,field.abi_type)
            .ok_or_else(||emission_verification("Unpack field ABI unavailable"))?)
    };
    let mut function=Function::new();function.signature=Signature::new(isa.default_call_conv());
    function.signature.params.push(AbiParam::new(pointer));function.signature.returns.push(AbiParam::new(pointer));
    let mut context=FunctionBuilderContext::new();
    {
        let mut builder=FunctionBuilder::new(&mut function,&mut context);
        let entry=builder.create_block();let rooted=builder.create_block();let identified=builder.create_block();
        let accepted=builder.create_block();let invalid=builder.create_block();let cleanup=builder.create_block();
        let failure=builder.create_block();builder.append_block_param(cleanup,pointer);
        builder.append_block_params_for_function_params(entry);builder.switch_to_block(entry);builder.seal_block(entry);
        let boxed=builder.block_params(entry)[0];
        let import=|builder:&mut FunctionBuilder<'_>,symbol:&str,parameters:&[cranelift_codegen::ir::Type],result:Option<cranelift_codegen::ir::Type>| {
            let mut signature=Signature::new(isa.default_call_conv());signature.params.extend(parameters.iter().copied().map(AbiParam::new));
            if let Some(result)=result{signature.returns.push(AbiParam::new(result));}
            let signature=builder.import_signature(signature);
            builder.func.import_function(ExtFuncData{name:ExternalName::testcase(symbol),signature,colocated:false,patchable:false})
        };
        let root=import(&mut builder,"gc_root_handle",&[pointer],Some(pointer));
        let resolve=import(&mut builder,"gc_resolve_handle",&[pointer],Some(pointer));
        let unroot=import(&mut builder,"gc_unroot_handle",&[pointer],None);
        let status=if checked{Some(import(&mut builder,"beskid_rt_v5_checked_scope_failure_reason",&[],Some(pointer)))}else{None};
        let valid=import(&mut builder,valid_symbol,&value_type.into_iter().collect::<Vec<_>>(),Some(pointer));
        let invalid_factory=import(&mut builder,invalid_symbol,&[],Some(pointer));
        let call=builder.ins().call(root,&[boxed]);let keeper=builder.inst_results(call)[0];
        let mut live=builder.ins().icmp_imm_u(IntCC::NotEqual,keeper,0);
        if let Some(status)=status{let call=builder.ins().call(status,&[]);let reason=builder.inst_results(call)[0];
            let clean=builder.ins().icmp_imm_u(IntCC::Equal,reason,0);live=builder.ins().band(live,clean);}
        builder.ins().brif(live,rooted,&[],failure,&[]);
        builder.switch_to_block(rooted);builder.seal_block(rooted);
        let call=builder.ins().call(resolve,&[keeper]);let current=builder.inst_results(call)[0];
        let live=builder.ins().icmp_imm_u(IntCC::NotEqual,current,0);builder.ins().brif(live,identified,&[],invalid,&[]);
        builder.switch_to_block(identified);builder.seal_block(identified);
        let actual=builder.ins().load(pointer,MemFlagsData::trusted(),current,0);
        let descriptor=builder.func.create_global_value(GlobalValueData::Symbol{name:ExternalName::testcase(&plan.descriptor_symbol),offset:0.into(),colocated:false,tls:false});
        let expected=builder.ins().symbol_value(pointer,descriptor);let same=builder.ins().icmp(IntCC::Equal,actual,expected);
        builder.ins().brif(same,accepted,&[],invalid,&[]);
        builder.switch_to_block(accepted);builder.seal_block(accepted);
        let arguments=if let Some(ty)=value_type {
            let offset=i32::try_from(field.field_offset).map_err(|_|emission_verification("Unpack field offset exceeds target displacement"))?;
            vec![builder.ins().load(ty,MemFlagsData::trusted(),current,offset)]
        }else{vec![]};
        let call=builder.ins().call(valid,&arguments);let result=builder.inst_results(call)[0];builder.ins().jump(cleanup,&[result.into()]);
        builder.switch_to_block(invalid);builder.seal_block(invalid);
        let call=builder.ins().call(invalid_factory,&[]);let result=builder.inst_results(call)[0];builder.ins().jump(cleanup,&[result.into()]);
        builder.switch_to_block(cleanup);builder.seal_block(cleanup);let mut result=builder.block_params(cleanup)[0];
        if let Some(status)=status{let call=builder.ins().call(status,&[]);let reason=builder.inst_results(call)[0];
            let clean=builder.ins().icmp_imm_u(IntCC::Equal,reason,0);let zero=builder.ins().iconst(pointer,0);result=builder.ins().select(clean,result,zero);}
        builder.ins().call(unroot,&[keeper]);builder.ins().return_(&[result]);
        builder.switch_to_block(failure);builder.seal_block(failure);builder.ins().call(unroot,&[keeper]);
        if checked{let zero=builder.ins().iconst(pointer,0);builder.ins().return_(&[zero]);}
        else{let call=builder.ins().call(invalid_factory,&[]);let result=builder.inst_results(call)[0];builder.ins().return_(&[result]);}
        builder.finalize(isa.frontend_config());
    }
    Ok(function)
}
