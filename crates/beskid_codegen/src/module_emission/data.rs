use std::collections::{HashMap, HashSet};

use beskid_analysis::types::TypeId;
use beskid_isle::AstNodeKey;
use beskid_queries::{child_nodes, closure_call_target, spawn_entry_validation};
use cranelift_codegen::ir::Endianness;
use cranelift_module::{DataDescription, DataId, Linkage, Module, ModuleResult};

use super::items::ResolvedSyntaxModuleItem;
use super::trampolines::{LambdaTrampoline, SpawnTrampoline};
use crate::CodegenInput;
use crate::aggregate_static::{AggregateStaticPlan, emit_aggregate_static_data};
use crate::array_static::{ArrayStaticPlan, emit_array_static_data};
use crate::closure_static::{ClosureStaticPlan, emit_closure_static_data};
use crate::{CodegenArtifact, TypeDescriptorData};

/// Cranelift [`DataId`] pair for a type: main descriptor blob and companion pointer-offset table.
#[derive(Debug, Clone)]
pub struct DescriptorHandles {
    pub descriptor: DataId,
    pub offsets: DataId,
}

pub(super) fn collect_array_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Vec<ArrayStaticPlan> {
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
    }
    nodes
        .into_iter()
        .filter_map(|key| input.array_static_plan(key).or_else(|| input.bulk_array_static_plan(key)))
        .collect()
}

pub(super) fn collect_aggregate_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Vec<AggregateStaticPlan> {
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
    }
    nodes
        .into_iter()
        .filter_map(|key| input.aggregate_static_plan(key).or_else(|| input.enum_static_plan(key)))
        .collect()
}

/// Collect source-proven closure static plans from generation-safe syntax facts.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
pub(super) fn collect_closure_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    trampolines: &[SpawnTrampoline],
    lambda_trampolines: &[LambdaTrampoline],
) -> Vec<ClosureStaticPlan> {
    let db = input.database();
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    let mut push_plan = |plan: ClosureStaticPlan| {
        if seen.insert(plan.lambda) {
            plans.push(plan);
        }
    };
    for trampoline in trampolines {
        if trampoline.closure_captures.is_some()
            && let Ok(Some(validation)) = spawn_entry_validation(db, trampoline.spawn)
            && let Some(authority) = input.closure_lowering_authority(trampoline.spawn, validation.target)
        {
            push_plan(authority.plan);
        }
    }
    for trampoline in lambda_trampolines {
        if trampoline.closure_captures.is_some()
            && let Some(authority) = input.closure_lowering_authority(trampoline.lambda, trampoline.lambda)
        {
            push_plan(authority.plan);
        }
    }
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(db, item.key, &mut visited, &mut nodes);
    }
    for key in nodes {
        if let Ok(Some(target)) = closure_call_target(db, key)
            && let Some(authority) = input.closure_lowering_authority(key, target.lambda)
        {
            push_plan(authority.plan);
        }
    }
    plans
}

fn collect_ast_nodes(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    nodes: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    nodes.push(key);
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_ast_nodes(db, child, visited, nodes);
        }
    }
}

/// Emit artifact-owned closure descriptor/pointer-map/allocation-request data.
pub fn emit_closure_static_plans<M: Module>(module: &mut M, artifact: &CodegenArtifact) -> ModuleResult<()> {
    for plan in &artifact.closure_static_plans {
        emit_closure_static_data(module, plan)?;
    }
    for plan in &artifact.aggregate_static_plans {
        emit_aggregate_static_data(module, plan)?;
    }
    for plan in &artifact.array_static_plans {
        emit_array_static_data(module, plan)?;
    }
    Ok(())
}

/// Define one module-local data object per entry in `artifact.string_literals`.
pub fn emit_string_literals<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
) -> ModuleResult<HashMap<String, DataId>> {
    let mut handles = HashMap::new();
    for (symbol, data) in &artifact.string_literals {
        let data_id = module.declare_data(symbol, Linkage::Local, false, false)?;
        let mut ctx = DataDescription::new();
        ctx.define(data.clone().into_boxed_slice());
        module.define_data(data_id, &ctx)?;
        handles.insert(symbol.clone(), data_id);
    }
    Ok(handles)
}

/// Emit descriptor and offset-table data for every type in `artifact.type_descriptors`.
pub fn emit_type_descriptors<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
) -> ModuleResult<HashMap<TypeId, DescriptorHandles>> {
    let mut handles = HashMap::new();
    for (type_id, descriptor) in &artifact.type_descriptors {
        let offsets_id = declare_descriptor_offsets(module, *type_id)?;
        let offsets_ctx = build_offsets_data(module, descriptor);
        module.define_data(offsets_id, &offsets_ctx)?;

        let descriptor_id = declare_descriptor(module, *type_id)?;
        let descriptor_ctx = build_descriptor_data(module, descriptor, offsets_id);
        module.define_data(descriptor_id, &descriptor_ctx)?;

        handles.insert(*type_id, DescriptorHandles { descriptor: descriptor_id, offsets: offsets_id });
    }
    Ok(handles)
}

pub(crate) fn descriptor_offsets_symbol_name(type_id: TypeId) -> String {
    format!("__beskid_type_offsets_{}", type_id.0)
}

pub(crate) fn descriptor_symbol_name(type_id: TypeId) -> String {
    format!("__beskid_type_desc_{}", type_id.0)
}

fn declare_descriptor_offsets<M: Module>(module: &mut M, type_id: TypeId) -> ModuleResult<DataId> {
    let name = descriptor_offsets_symbol_name(type_id);
    module.declare_data(&name, Linkage::Local, false, false)
}

fn declare_descriptor<M: Module>(module: &mut M, type_id: TypeId) -> ModuleResult<DataId> {
    let name = descriptor_symbol_name(type_id);
    module.declare_data(&name, Linkage::Local, false, false)
}

fn build_offsets_data<M: Module>(module: &M, descriptor: &TypeDescriptorData) -> DataDescription {
    let mut ctx = DataDescription::new();
    let ptr_size = module.isa().pointer_bytes();
    let little_endian = matches!(module.isa().endianness(), Endianness::Little);

    let mut bytes = Vec::with_capacity(descriptor.pointer_offsets.len() * ptr_size as usize);
    for offset in &descriptor.pointer_offsets {
        write_usize(&mut bytes, *offset, ptr_size, little_endian);
    }
    ctx.define(bytes.into_boxed_slice());
    ctx
}

fn build_descriptor_data<M: Module>(
    module: &mut M,
    descriptor: &TypeDescriptorData,
    offsets_id: DataId,
) -> DataDescription {
    let ptr_size = module.isa().pointer_bytes();
    let little_endian = matches!(module.isa().endianness(), Endianness::Little);
    let usize_align = ptr_size as usize;
    let u32_align = 4usize;

    let mut ctx = DataDescription::new();
    let mut bytes = Vec::new();

    let _size_offset = push_usize(&mut bytes, descriptor.size, ptr_size, little_endian, usize_align);
    let _align_offset = push_usize(&mut bytes, descriptor.align, ptr_size, little_endian, usize_align);
    let _ptr_count_offset = push_u32(&mut bytes, descriptor.pointer_offsets.len() as u32, little_endian, u32_align);

    pad_to_alignment(&mut bytes, usize_align);
    let ptr_offsets_offset = bytes.len();
    bytes.extend(std::iter::repeat_n(0u8, usize_align));

    pad_to_alignment(&mut bytes, usize_align);
    let _name_offset = bytes.len();
    bytes.extend(std::iter::repeat_n(0u8, usize_align));

    ctx.define(bytes.into_boxed_slice());
    let gv = module.declare_data_in_data(offsets_id, &mut ctx);
    ctx.write_data_addr(ptr_offsets_offset as u32, gv, 0);
    ctx
}

fn write_usize(buf: &mut Vec<u8>, value: usize, ptr_size: u8, little_endian: bool) {
    match (ptr_size, little_endian) {
        (4, true) => buf.extend_from_slice(&(value as u32).to_le_bytes()),
        (4, false) => buf.extend_from_slice(&(value as u32).to_be_bytes()),
        (8, true) => buf.extend_from_slice(&(value as u64).to_le_bytes()),
        (8, false) => buf.extend_from_slice(&(value as u64).to_be_bytes()),
        _ => panic!("unsupported pointer size {ptr_size}"),
    }
}

fn push_usize(buf: &mut Vec<u8>, value: usize, ptr_size: u8, little_endian: bool, align: usize) -> usize {
    pad_to_alignment(buf, align);
    let offset = buf.len();
    write_usize(buf, value, ptr_size, little_endian);
    offset
}

fn push_u32(buf: &mut Vec<u8>, value: u32, little_endian: bool, align: usize) -> usize {
    pad_to_alignment(buf, align);
    let offset = buf.len();
    if little_endian {
        buf.extend_from_slice(&value.to_le_bytes());
    } else {
        buf.extend_from_slice(&value.to_be_bytes());
    }
    offset
}

fn pad_to_alignment(buf: &mut Vec<u8>, align: usize) {
    let padding = (align - (buf.len() % align)) % align;
    if padding > 0 {
        buf.extend(std::iter::repeat_n(0u8, padding));
    }
}
