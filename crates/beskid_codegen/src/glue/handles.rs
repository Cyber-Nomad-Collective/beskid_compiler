//! Source-derived managed nominal boxing plans for logical opaque tokens.
//! These plans are private lowering facts; JSON cannot issue a GC descriptor.
use crate::{CodegenInput, aggregate_static::AggregateStaticPlan, backend::BackendError};
use beskid_queries::{AstNodeKey, GlueLogicalType, SemanticTypeId};
use super::artifact::digest;

/// Constructed only by current-source emission into an actual object module.
/// Names are evidence for private producer admission, not caller-mintable plans.
pub struct EmittedHandleTransport {
    binding:AstNodeKey,position:Option<usize>,brand:String,constructor:String,reader:String,descriptor:String,
    functions:Vec<cranelift_module::FuncId>,
}
impl EmittedHandleTransport {
    pub fn binding(&self)->AstNodeKey{self.binding}
    pub fn position(&self)->Option<usize>{self.position}
    pub fn brand_sha256(&self)->&str{&self.brand}
    pub fn constructor(&self)->&str{&self.constructor}
    pub fn reader(&self)->&str{&self.reader}
    pub fn descriptor(&self)->&str{&self.descriptor}
    pub fn functions(&self)->&[cranelift_module::FuncId]{&self.functions}
}

/// Emit normalized nominal token boxing after the source artifact's sole data
/// pass. The constructor never interprets a u64 token as a managed address.
pub fn emit_source_handle_transports<M:cranelift_module::Module>(
    module:&mut M,input:&CodegenInput<'_>,bindings:&[AstNodeKey],
)->cranelift_module::ModuleResult<Vec<EmittedHandleTransport>> {
    use cranelift_codegen::ir::{AbiParam,InstBuilder,MemFlagsData,types,condcodes::IntCC};
    use cranelift_frontend::{FunctionBuilder,FunctionBuilderContext};
    use cranelift_module::{FuncOrDataId,Linkage,ModuleError};
    let error=|message:&str|ModuleError::Backend(anyhow::anyhow!("{message}"));
    let mut plans=Vec::new();
    for binding in bindings {for plan in source_handle_box_plans(input,*binding).map_err(|cause|error(&cause.to_string()))? {
        plans.push((*binding,plan));
    }}
    let pointer=module.target_config().pointer_type();
    let header=input.abi_manifest().layouts.iter().find(|layout|layout.name=="BeskidObjectHeader")
        .ok_or_else(||error("opaque object header unavailable"))?;
    let descriptor_offset=header.fields.iter().find(|field|field.name=="descriptor")
        .and_then(|field|i32::try_from(field.offset).ok()).ok_or_else(||error("opaque descriptor offset unavailable"))?;
    let mut emitted:Vec<EmittedHandleTransport>=Vec::new();
    for (binding,plan) in plans {
        if let Some(previous)=emitted.iter().find(|previous|previous.brand==plan.brand_sha256) {
            let repeated=EmittedHandleTransport{binding,position:plan.position,brand:previous.brand.clone(),
                constructor:previous.constructor.clone(),reader:previous.reader.clone(),descriptor:previous.descriptor.clone(),functions:previous.functions.clone()};
            emitted.push(repeated);continue;
        }
        let data=|name:&str|match module.get_name(name){Some(FuncOrDataId::Data(id))=>Ok(id),_=>Err(error("opaque source data was not emitted"))};
        let request=data(&plan.plan.allocation_request_symbol)?;
        let descriptor=data(&plan.plan.descriptor_symbol)?;
        let offset=i32::try_from(plan.plan.fields[0].field_offset).map_err(|_|error("opaque field offset overflow"))?;
        let constructor=format!("beskid_glue_handle_{}_construct",plan.brand_sha256);
        let reader=format!("beskid_glue_handle_{}_read",plan.brand_sha256);
        if module.get_name(&constructor).is_some() || module.get_name(&reader).is_some() {
            return Err(error("opaque transport symbol conflicts with an existing declaration"));
        }
        let mut unary=module.make_signature();unary.params.push(AbiParam::new(pointer));unary.returns.push(AbiParam::new(pointer));
        let allocate=module.declare_function("beskid_rt_v5_managed_object_try_allocate",Linkage::Import,&unary)?;
        let root=module.declare_function("beskid_rt_v5_gc_try_root_handle",Linkage::Import,&unary)?;
        let resolve=module.declare_function("beskid_rt_v5_gc_resolve_handle",Linkage::Import,&unary)?;
        let mut void=module.make_signature();void.params.push(AbiParam::new(pointer));
        let unroot=module.declare_function("gc_unroot_handle",Linkage::Import,&void)?;
        let mut context=module.make_context();context.func.signature.params.push(AbiParam::new(types::I64));context.func.signature.returns.push(AbiParam::new(pointer));
        let construct=module.declare_function(&constructor,Linkage::Export,&context.func.signature)?;
        let allocate=module.declare_func_in_func(allocate,&mut context.func);
        let request=module.declare_data_in_func(request,&mut context.func);
        let mut frontend=FunctionBuilderContext::new();
        {
            let mut builder=FunctionBuilder::new(&mut context.func,&mut frontend);
            let entry=builder.create_block();let ready=builder.create_block();let failed=builder.create_block();
            builder.append_block_params_for_function_params(entry);builder.switch_to_block(entry);builder.seal_block(entry);
            let token=builder.block_params(entry)[0];let request=builder.ins().symbol_value(pointer,request);
            let call=builder.ins().call(allocate,&[request]);let value=builder.inst_results(call)[0];
            let live=builder.ins().icmp_imm_u(IntCC::NotEqual,value,0);builder.ins().brif(live,ready,&[],failed,&[]);
            builder.switch_to_block(ready);builder.seal_block(ready);
            builder.ins().store(MemFlagsData::new(),token,value,offset);builder.ins().return_(&[value]);
            builder.switch_to_block(failed);builder.seal_block(failed);let zero=builder.ins().iconst(pointer,0);builder.ins().return_(&[zero]);
            builder.finalize(module.target_config());
        }
        module.define_function(construct,&mut context)?;
        let mut context=module.make_context();context.func.signature.params.extend([AbiParam::new(pointer),AbiParam::new(pointer)]);
        context.func.signature.returns.push(AbiParam::new(types::I32));
        let read=module.declare_function(&reader,Linkage::Export,&context.func.signature)?;
        let root=module.declare_func_in_func(root,&mut context.func);let resolve=module.declare_func_in_func(resolve,&mut context.func);
        let unroot=module.declare_func_in_func(unroot,&mut context.func);let descriptor=module.declare_data_in_func(descriptor,&mut context.func);
        let mut frontend=FunctionBuilderContext::new();
        {
            let mut builder=FunctionBuilder::new(&mut context.func,&mut frontend);
            let entry=builder.create_block();let aligned=builder.create_block();let rooted=builder.create_block();let identified=builder.create_block();let accepted=builder.create_block();let cleanup=builder.create_block();let failed=builder.create_block();
            builder.append_block_param(cleanup,types::I32);builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);builder.seal_block(entry);
            let value=builder.block_params(entry)[0];let output=builder.block_params(entry)[1];
            let nonzero=builder.ins().icmp_imm_u(IntCC::NotEqual,output,0);let mask=builder.ins().band_imm(output,7);
            let alignment=builder.ins().icmp_imm_u(IntCC::Equal,mask,0);let valid=builder.ins().band(nonzero,alignment);
            builder.ins().brif(valid,aligned,&[],failed,&[]);
            builder.switch_to_block(aligned);builder.seal_block(aligned);let zero=builder.ins().iconst(types::I64,0);builder.ins().store(MemFlagsData::new(),zero,output,0);
            let call=builder.ins().call(root,&[value]);let keeper=builder.inst_results(call)[0];let live=builder.ins().icmp_imm_u(IntCC::NotEqual,keeper,0);
            builder.ins().brif(live,rooted,&[],failed,&[]);
            builder.switch_to_block(rooted);builder.seal_block(rooted);let call=builder.ins().call(resolve,&[keeper]);let value=builder.inst_results(call)[0];
            let live=builder.ins().icmp_imm_u(IntCC::NotEqual,value,0);let invalid=builder.ins().iconst(types::I32,1);
            builder.ins().brif(live,identified,&[],cleanup,&[invalid.into()]);
            builder.switch_to_block(identified);builder.seal_block(identified);
            let actual=builder.ins().load(pointer,MemFlagsData::new(),value,descriptor_offset);let expected=builder.ins().symbol_value(pointer,descriptor);
            let matches=builder.ins().icmp(IntCC::Equal,actual,expected);builder.ins().brif(matches,accepted,&[],cleanup,&[invalid.into()]);
            builder.switch_to_block(accepted);builder.seal_block(accepted);let token=builder.ins().load(types::I64,MemFlagsData::new(),value,offset);
            builder.ins().store(MemFlagsData::new(),token,output,0);let success=builder.ins().iconst(types::I32,0);builder.ins().jump(cleanup,&[success.into()]);
            builder.switch_to_block(cleanup);builder.seal_block(cleanup);let status=builder.block_params(cleanup)[0];builder.ins().call(unroot,&[keeper]);builder.ins().return_(&[status]);
            builder.switch_to_block(failed);builder.seal_block(failed);let invalid=builder.ins().iconst(types::I32,1);builder.ins().return_(&[invalid]);
            builder.finalize(module.target_config());
        }
        module.define_function(read,&mut context)?;
        emitted.push(EmittedHandleTransport{binding,position:plan.position,brand:plan.brand_sha256,constructor,reader,
            descriptor:plan.plan.descriptor_getter.ok_or_else(||error("opaque descriptor getter absent"))?,functions:vec![construct,read]});
    }
    Ok(emitted)
}

pub(crate) struct HandleBoxPlan {
    pub position: Option<usize>,
    pub brand_sha256: String,
    pub nullable: bool,
    pub library: String,
    pub plan: AggregateStaticPlan,
}

pub(crate) fn source_handle_box_plans(
    input: &CodegenInput<'_>, binding: AstNodeKey,
) -> Result<Vec<HandleBoxPlan>, BackendError> {
    if binding.generation != input.typed_program().generation {
        return Err(BackendError::RustGlueFact {key:binding,message:"stale opaque binding generation".into()});
    }
    let Some(fact) = beskid_queries::glue_binding(input.database(),binding)
        .map_err(|error|BackendError::RustGlueFact {key:binding,message:error.to_string()})? else {return Ok(Vec::new())};
    let mut result=Vec::new();
    for (position,ty) in fact.parameters.iter().enumerate().map(|(index,p)|(Some(index),&p.ty))
        .chain(std::iter::once((None,&fact.result))) {
        let GlueLogicalType::Handle(brand)=ty else {continue};
        let layout=input.aggregate_object_layout(brand.declaration)
            .ok_or_else(||BackendError::UnsupportedRustGlueType("opaque nominal layout unavailable".into()))?;
        if layout.fields.len()!=1 || layout.fields[0].abi_type!=SemanticTypeId::U64 ||
            !layout.pointer_map_offsets.is_empty() {
            return Err(BackendError::UnsupportedRustGlueType("opaque nominal must retain its exact untraced u64 field".into()));
        }
        let graph=beskid_queries::glue_handle_shape(input.database(),binding,position)
            .map_err(|error|BackendError::RustGlueFact {key:binding,message:error.to_string()})?
            .ok_or_else(||BackendError::UnsupportedRustGlueType("opaque canonical shape unavailable".into()))?;
        let shape=input.compile_stable_shape_graph(&graph,"1",1024*1024)
            .map_err(BackendError::UnsupportedRustGlueType)?;
        // One shared portable graph issuer owns nominal/package/applied-argument
        // identity. The opaque profile binds only its additional ownership policy.
        let mut recipe=b"beskid.glue.opaque/1\0".to_vec();
        recipe.extend_from_slice(&(shape.signature().len() as u64).to_le_bytes());
        recipe.extend_from_slice(shape.signature());
        recipe.extend_from_slice(&(brand.library.len() as u64).to_le_bytes());
        recipe.extend_from_slice(brand.library.as_bytes());
        recipe.push(u8::from(brand.nullable));
        let identity=digest(&recipe);
        result.push(HandleBoxPlan {position,brand_sha256:identity.clone(),nullable:brand.nullable,
            library:brand.library.clone(),plan:AggregateStaticPlan {
                literal:brand.declaration,
                descriptor_symbol:format!("__beskid_glue_handle_{identity}_descriptor"),
                pointer_map_symbol:format!("__beskid_glue_handle_{identity}_pointer_map"),
                allocation_request_symbol:format!("__beskid_glue_handle_{identity}_request"),
                object_size:layout.object_size,object_alignment:layout.object_alignment,
                pointer_map_offsets:layout.pointer_map_offsets,fields:layout.fields,
                descriptor_flags:0,
                descriptor_getter:Some(format!("beskid_glue_handle_{identity}_descriptor")),
            }});
    }
    Ok(result)
}
