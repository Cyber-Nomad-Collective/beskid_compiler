//! ABI-v5 static allocation metadata for managed aggregate literals.

use std::sync::Arc;

use beskid_queries::{
    AggregateFieldShape, AstNodeKey, SemanticTypeId, aggregate_layout, aggregate_literal_declaration, enum_layout,
    enum_match,
};
use cranelift_module::{DataDescription, DataId, Linkage, Module, ModuleError, ModuleResult};

use crate::CodegenInput;

/// Canonical ABI-v5 allocation entrypoint for managed aggregate values.
pub const ABI_V5_MANAGED_OBJECT_ALLOCATE: &str = "beskid_rt_v5_managed_object_allocate";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggregateStaticField {
    pub abi_type: SemanticTypeId,
    pub field_offset: u64,
}

/// Header-relative object layout for a managed aggregate declaration.
///
/// This is the single source of truth for managed field offsets: allocation planning
/// (`aggregate_static_plan`) and field access lowering must agree byte-for-byte, so both derive
/// their offsets from here rather than recomputing them. Offsets are measured from the start of the
/// object, i.e. they include the leading `BeskidObjectHeader`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateObjectLayout {
    pub declaration: AstNodeKey,
    pub object_size: u64,
    pub object_alignment: u64,
    pub pointer_map_offsets: Arc<[u64]>,
    pub fields: Arc<[AggregateStaticField]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateStaticPlan {
    pub literal: AstNodeKey,
    pub descriptor_symbol: String,
    pub pointer_map_symbol: String,
    pub allocation_request_symbol: String,
    pub object_size: u64,
    pub object_alignment: u64,
    pub pointer_map_offsets: Arc<[u64]>,
    pub fields: Arc<[AggregateStaticField]>,
}

pub fn emit_aggregate_static_data<M: Module>(
    module: &mut M,
    plan: &AggregateStaticPlan,
) -> ModuleResult<(DataId, DataId, DataId)> {
    let pointer_map = module.declare_data(&plan.pointer_map_symbol, Linkage::Local, false, false)?;
    let descriptor = module.declare_data(&plan.descriptor_symbol, Linkage::Local, false, false)?;
    let request = module.declare_data(&plan.allocation_request_symbol, Linkage::Local, false, false)?;
    let mut pointer_map_bytes = Vec::with_capacity(plan.pointer_map_offsets.len().max(1) * 8);
    if plan.pointer_map_offsets.is_empty() {
        pointer_map_bytes.extend_from_slice(&0u64.to_le_bytes());
    } else {
        for offset in plan.pointer_map_offsets.iter() {
            pointer_map_bytes.extend_from_slice(&offset.to_le_bytes());
        }
    }
    let mut pointer_map_data = DataDescription::new();
    pointer_map_data.define(pointer_map_bytes.into_boxed_slice());
    module.define_data(pointer_map, &pointer_map_data)?;
    let mut descriptor_bytes = vec![0u8; 40];
    write_word(&mut descriptor_bytes, 0, plan.object_size)?;
    write_word(&mut descriptor_bytes, 8, plan.object_alignment)?;
    write_word(
        &mut descriptor_bytes,
        24,
        u64::try_from(plan.pointer_map_offsets.len())
            .map_err(|_| ModuleError::Backend(anyhow::anyhow!("aggregate pointer-map length exceeds ABI word")))?,
    )?;
    write_word(&mut descriptor_bytes, 32, 1)?; // flags bit 0 = IS_AGGREGATE
    let mut descriptor_data = DataDescription::new();
    descriptor_data.define(descriptor_bytes.into_boxed_slice());
    let pointer_map_address = module.declare_data_in_data(pointer_map, &mut descriptor_data);
    descriptor_data.write_data_addr(16, pointer_map_address, 0);
    module.define_data(descriptor, &descriptor_data)?;
    let mut request_bytes = vec![0u8; 24];
    write_word(&mut request_bytes, 0, plan.object_size)?;
    write_word(&mut request_bytes, 8, plan.object_alignment)?;
    let mut request_data = DataDescription::new();
    request_data.define(request_bytes.into_boxed_slice());
    let descriptor_address = module.declare_data_in_data(descriptor, &mut request_data);
    request_data.write_data_addr(16, descriptor_address, 0);
    module.define_data(request, &request_data)?;
    Ok((pointer_map, descriptor, request))
}

fn write_word(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), ModuleError> {
    let destination = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| ModuleError::Backend(anyhow::anyhow!("aggregate static-data layout overflow")))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

impl CodegenInput<'_> {
    /// Compute the header-relative field layout of a managed aggregate declaration.
    ///
    /// Field access lowering consumes this directly so that reads and writes address the same bytes
    /// the allocation plan reserved.
    pub fn aggregate_object_layout(&self, declaration: AstNodeKey) -> Option<AggregateObjectLayout> {
        let aggregate = aggregate_layout(self.database(), declaration).ok().flatten()?;
        let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        if header.size < 16 || !valid_alignment(header.alignment) {
            return None;
        }
        let mut size = header.size;
        let mut alignment = header.alignment;
        let mut pointer_map_offsets = Vec::new();
        let mut fields = Vec::with_capacity(aggregate.fields.len());
        for (_, shape) in aggregate.fields.iter() {
            let abi_type = match shape {
                AggregateFieldShape::Scalar(semantic) => *semantic,
                AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
            };
            let (field_size, field_alignment, pointer) = scalar_layout(self.target().pointer_width, abi_type)?;
            size = align_to(size, field_alignment)?;
            let field_offset = size;
            size = size.checked_add(field_size)?;
            alignment = alignment.max(field_alignment);
            if pointer {
                pointer_map_offsets.push(field_offset);
            }
            fields.push(AggregateStaticField { abi_type, field_offset });
        }
        let object_size = align_to(size, alignment)?;
        Some(AggregateObjectLayout {
            declaration,
            object_size,
            object_alignment: alignment,
            pointer_map_offsets: pointer_map_offsets.into(),
            fields: fields.into(),
        })
    }

    pub fn aggregate_static_plan(&self, literal: AstNodeKey) -> Option<AggregateStaticPlan> {
        let declaration = aggregate_literal_declaration(self.database(), literal).ok().flatten()?;
        let descriptor = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidTypeDescriptor")?;
        let request = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAllocationRequest")?;
        if descriptor.size != 40 || descriptor.alignment != 8 || request.size != 24 || request.alignment != 8 {
            return None;
        }
        let layout = self.aggregate_object_layout(declaration)?;
        let unit = self
            .typed_program()
            .assembly
            .units()
            .iter()
            .position(|unit| paths_match(&unit.path, literal.unit.path(self.database())))?;
        let identity = format!("u{unit}_g{}_n{}", literal.generation.0, literal.node.0);
        Some(AggregateStaticPlan {
            literal,
            descriptor_symbol: format!("__beskid_aggregate_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_aggregate_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_aggregate_allocation_request_{identity}"),
            object_size: layout.object_size,
            object_alignment: layout.object_alignment,
            pointer_map_offsets: layout.pointer_map_offsets,
            fields: layout.fields,
        })
    }

    /// Produce the managed allocation metadata for an enum constructor. Enum values are references,
    /// so their tag and payload must not be backed by the constructor's stack frame.
    pub fn enum_static_plan(&self, literal: AstNodeKey) -> Option<AggregateStaticPlan> {
        let layout = enum_layout(self.database(), literal)
            .ok()
            .flatten()
            .or_else(|| enum_match(self.database(), literal).ok().flatten().map(|fact| fact.layout))?;
        let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        if header.size < 16 || !valid_alignment(header.alignment) {
            return None;
        }
        let mut size = header.size;
        let mut alignment = header.alignment.max(4);
        size = align_to(size, 4)?;
        let tag_offset = size;
        size = size.checked_add(4)?;
        let mut payload_offset = size;
        let mut payload_size = 0_u64;
        let mut payload_alignment = 1_u64;
        let mut payload_is_pointer = false;
        for variant in layout.variants.iter() {
            let shape = match variant.fields.as_ref() {
                [] => continue,
                [(_, shape)] => shape,
                _ => return None,
            };
            let (field_size, field_alignment, pointer) = match shape {
                AggregateFieldShape::Scalar(semantic) => scalar_layout(self.target().pointer_width, *semantic)?,
                AggregateFieldShape::Nominal(_) => scalar_layout(self.target().pointer_width, SemanticTypeId::POINTER)?,
            };
            payload_size = payload_size.max(field_size);
            payload_alignment = payload_alignment.max(field_alignment);
            payload_is_pointer |= pointer;
        }
        let mut fields = vec![AggregateStaticField { abi_type: SemanticTypeId::I32, field_offset: tag_offset }];
        if payload_size > 0 {
            alignment = alignment.max(payload_alignment);
            payload_offset = align_to(payload_offset, payload_alignment)?;
            size = payload_offset.checked_add(payload_size)?;
            fields.push(AggregateStaticField {
                abi_type: if payload_is_pointer { SemanticTypeId::POINTER } else { SemanticTypeId::I64 },
                field_offset: payload_offset,
            });
        }
        let object_size = align_to(size, alignment)?;
        let pointer_map_offsets = payload_is_pointer.then_some(payload_offset).into_iter().collect::<Vec<_>>();
        let unit = self.typed_program().assembly.units().iter().position(|unit| {
            paths_match(&unit.path, literal.unit.path(self.database()))
        })?;
        let identity = format!("enum_u{unit}_g{}_n{}", literal.generation.0, literal.node.0);
        Some(AggregateStaticPlan {
            literal,
            descriptor_symbol: format!("__beskid_aggregate_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_aggregate_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_aggregate_allocation_request_{identity}"),
            object_size,
            object_alignment: alignment,
            pointer_map_offsets: pointer_map_offsets.into(),
            fields: fields.into(),
        })
    }
}

fn scalar_layout(pointer_width: u8, ty: SemanticTypeId) -> Option<(u64, u64, bool)> {
    let pointer = u64::from(pointer_width.checked_div(8)?);
    match ty {
        SemanticTypeId::BOOL | SemanticTypeId::U8 => Some((1, 1, false)),
        SemanticTypeId::I32 | SemanticTypeId::CHAR => Some((4, 4, false)),
        SemanticTypeId::I64 | SemanticTypeId::F64 => Some((8, 8, false)),
        SemanticTypeId::WORD => Some((pointer, pointer, false)),
        SemanticTypeId::POINTER | SemanticTypeId::STRING => Some((pointer, pointer, true)),
        _ => None,
    }
}
fn valid_alignment(value: u64) -> bool {
    value.is_power_of_two() && value > 0
}
fn align_to(value: u64, alignment: u64) -> Option<u64> {
    valid_alignment(alignment).then_some(())?;
    value.checked_add(alignment - 1).map(|value| value & !(alignment - 1))
}
fn paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}
