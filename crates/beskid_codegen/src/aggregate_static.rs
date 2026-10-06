//! ABI-v5 static allocation metadata for managed aggregate literals.

use std::sync::Arc;

use beskid_queries::{
    AggregateFieldAccess, AggregateFieldShape, AggregateLayoutFact, AstNodeKey, GenericSpecializationInstance,
    IndexedNodeKind, SemanticTypeId, aggregate_layout, aggregate_literal_declaration, aggregate_literal_layout,
    aggregate_literal_specialization, child_nodes, composition_injection_field, composition_registration,
    enum_constructor_specialization, enum_layout, enum_match, event_field_layout, generic_specialization_identity,
    node_kind,
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
    /// Physical managed pointer slots for compiler-resolved injected fields.
    pub injected_fields: Arc<[(AstNodeKey, u64)]>,
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
    pub descriptor_flags: u64,
    pub descriptor_getter: Option<String>,
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
    // Flag bit 0 is reserved for the variable-sized array object descriptor. Ordinary
    // aggregates (including enum payload objects) use the unflagged descriptor shape.
    write_word(&mut descriptor_bytes, 32, plan.descriptor_flags)?;
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
    if let Some(symbol) = &plan.descriptor_getter {
        use cranelift_codegen::ir::{AbiParam, InstBuilder};
        use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
        let pointer_type = module.target_config().pointer_type();
        let mut context = module.make_context();
        context.func.signature.returns.push(AbiParam::new(pointer_type));
        let function = module.declare_function(symbol, Linkage::Export, &context.func.signature)?;
        let data = module.declare_data_in_func(descriptor, &mut context.func);
        let mut frontend = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
            let entry = builder.create_block();
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let address = builder.ins().symbol_value(pointer_type, data);
            builder.ins().return_(&[address]);
            builder.finalize(module.target_config());
        }
        module.define_function(function, &mut context)?;
    }

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
    /// Descriptor-backed default construction for one exact frozen registry entry.
    /// A registration with value fields needs a source constructor and is not fabricated here.
    pub fn composition_registration_static_plan(&self, registration_id: u32) -> Option<AggregateStaticPlan> {
        let (_, snapshot) = self.composition_authority()?;
        let registration = snapshot.registrations.iter().find(|registration| registration.id == registration_id)?;
        let site = AstNodeKey {
            unit: self.typed_program().entry,
            generation: self.typed_program().generation,
            node: registration.source_node_id,
        };
        let fact = composition_registration(self.database(), site).ok().flatten()?;
        (fact.site == site && fact.implementation.as_ref() == registration.implementation).then_some(())?;
        let layout = self.aggregate_object_layout(fact.declaration)?;
        layout.fields.is_empty().then_some(())?;
        let descriptor = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidTypeDescriptor")?;
        let request = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAllocationRequest")?;
        (descriptor.size == 40 && descriptor.alignment == 8 && request.size == 24 && request.alignment == 8)
            .then_some(())?;
        let unit = self
            .typed_program()
            .assembly
            .units
            .iter()
            .position(|unit| beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == site.unit)?;
        let identity = format!("{}_u{unit}_g{}_r{registration_id}", artifact_namespace(self), site.generation.0);
        Some(AggregateStaticPlan {
            descriptor_flags: 0,
            descriptor_getter: None,
            literal: site,
            descriptor_symbol: format!("__beskid_composition_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_composition_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_composition_request_{identity}"),
            object_size: layout.object_size,
            object_alignment: layout.object_alignment,
            pointer_map_offsets: layout.pointer_map_offsets,
            fields: layout.fields,
        })
    }

    /// Invocation bootstrap for an exact field-free source factory declaration.
    /// Stateful receivers must use their source factory, never zero-filled fields.
    pub(crate) fn native_empty_factory_plan(&self, declaration: AstNodeKey) -> Option<AggregateStaticPlan> {
        let layout = self.aggregate_object_layout(declaration)?;
        if !layout.fields.is_empty() || !layout.injected_fields.is_empty() {
            return None;
        }
        let descriptor = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidTypeDescriptor")?;
        let request = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAllocationRequest")?;
        if descriptor.size != 40 || request.size != 24 {
            return None;
        }
        let unit = self.typed_program().assembly.units.iter().position(|unit| {
            beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == declaration.unit
        })?;
        let identity =
            format!("{}_u{unit}_g{}_n{}", artifact_namespace(self), declaration.generation.0, declaration.node.0);
        Some(AggregateStaticPlan {
            descriptor_flags: 0,
            descriptor_getter: None,
            literal: declaration,
            descriptor_symbol: format!("__beskid_mod_factory_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_mod_factory_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_mod_factory_request_{identity}"),
            object_size: layout.object_size,
            object_alignment: layout.object_alignment,
            pointer_map_offsets: layout.pointer_map_offsets,
            fields: layout.fields,
        })
    }

    /// Allocate the ordinary source-defined Fiber<T> shape; no parallel handle layout.
    pub fn spawn_handle_static_plan(&self, spawn: AstNodeKey) -> Option<AggregateStaticPlan> {
        let handle = beskid_queries::spawn_handle_type(self.database(), spawn).ok().flatten()?;
        let layout = self.aggregate_object_layout(handle.declaration)?;
        if layout.fields.len() != 1 || layout.fields[0].abi_type != SemanticTypeId::I64 {
            return None;
        }
        let unit = self
            .typed_program()
            .assembly
            .units
            .iter()
            .position(|unit| beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == spawn.unit)?;
        let identity = format!("{}_u{unit}_g{}_n{}", artifact_namespace(self), spawn.generation.0, spawn.node.0);
        Some(AggregateStaticPlan {
            descriptor_flags: 0,
            descriptor_getter: None,
            literal: spawn,
            descriptor_symbol: format!("__beskid_fiber_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_fiber_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_fiber_request_{identity}"),
            object_size: layout.object_size,
            object_alignment: layout.object_alignment,
            pointer_map_offsets: layout.pointer_map_offsets,
            fields: layout.fields,
        })
    }

    /// Lay out the managed start environment that carries eager `spawn Entry(args)` arguments.
    ///
    /// One field per entry parameter, in parameter order, typed by the entry's ABI signature.
    /// Every pointer-ABI field is in the pointer map, exactly like aggregate fields, so managed
    /// arguments stay traced while the runtime roots the environment for the fiber's lifetime.
    /// Zero-argument entries, illegal spawns, and unsupported parameter ABIs have no plan.
    pub fn spawn_argument_static_plan(&self, spawn: AstNodeKey) -> Option<AggregateStaticPlan> {
        let validation = beskid_queries::spawn_entry_validation(self.database(), spawn).ok().flatten()?;
        if !validation.is_legal_entry || validation.arguments.is_empty() {
            return None;
        }
        let callable = validation.callable.as_ref()?;
        if callable.parameters.len() != validation.arguments.len() {
            return None;
        }
        let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let descriptor = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidTypeDescriptor")?;
        let request = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAllocationRequest")?;
        if header.size < 16
            || !valid_alignment(header.alignment)
            || descriptor.size != 40
            || descriptor.alignment != 8
            || request.size != 24
            || request.alignment != 8
        {
            return None;
        }
        let mut size = header.size;
        let mut alignment = header.alignment;
        let mut pointer_map_offsets = Vec::new();
        let mut fields = Vec::with_capacity(callable.parameters.len());
        for abi_type in callable.parameters.iter().copied() {
            let scalar = abi_type.scalar_abi_layout(self.target().pointer_width)?;
            size = align_to(size, scalar.alignment)?;
            let field_offset = size;
            size = size.checked_add(scalar.size)?;
            alignment = alignment.max(scalar.alignment);
            if scalar.is_pointer {
                pointer_map_offsets.push(field_offset);
            }
            fields.push(AggregateStaticField { abi_type, field_offset });
        }
        let object_size = align_to(size, alignment)?;
        let unit = self
            .typed_program()
            .assembly
            .units
            .iter()
            .position(|unit| beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == spawn.unit)?;
        let identity = format!("{}_u{unit}_g{}_n{}", artifact_namespace(self), spawn.generation.0, spawn.node.0);
        Some(AggregateStaticPlan {
            descriptor_flags: 0,
            descriptor_getter: None,
            literal: spawn,
            descriptor_symbol: format!("__beskid_spawn_arguments_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_spawn_arguments_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_spawn_arguments_request_{identity}"),
            object_size,
            object_alignment: alignment,
            pointer_map_offsets: pointer_map_offsets.into(),
            fields: fields.into(),
        })
    }

    /// Compute the header-relative field layout of a managed aggregate declaration.
    ///
    /// Field access lowering consumes this directly so that reads and writes address the same bytes
    /// the allocation plan reserved.
    pub fn aggregate_object_layout(&self, declaration: AstNodeKey) -> Option<AggregateObjectLayout> {
        if beskid_queries::runtime_managed_opaque_kind(self.database(), declaration).is_some() {
            return None;
        }
        let aggregate = aggregate_layout(self.database(), declaration).ok().flatten()?;
        self.aggregate_object_layout_from_fact(declaration, &aggregate)
    }

    pub fn aggregate_object_layout_for_access(&self, access: &AggregateFieldAccess) -> Option<AggregateObjectLayout> {
        self.aggregate_object_layout_from_fact(access.declaration, &access.layout)
    }

    pub(crate) fn aggregate_object_layout_from_fact(
        &self,
        declaration: AstNodeKey,
        aggregate: &AggregateLayoutFact,
    ) -> Option<AggregateObjectLayout> {
        let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        if header.size != 16 || !valid_alignment(header.alignment) {
            return None;
        }
        let mut size = header.size;
        let mut alignment = header.alignment;
        let mut pointer_map_offsets = Vec::new();
        let mut fields = Vec::with_capacity(aggregate.fields.len());
        let mut injected_fields = Vec::new();
        for (_, shape) in aggregate.fields.iter() {
            let abi_type = match shape {
                AggregateFieldShape::Scalar(semantic) | AggregateFieldShape::ManagedReference(semantic) => *semantic,
                AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
            };
            if abi_type == SemanticTypeId::UNIT {
                fields.push(AggregateStaticField { abi_type, field_offset: size });
                continue;
            }
            let scalar = abi_type.scalar_abi_layout(self.target().pointer_width)?;
            size = align_to(size, scalar.alignment)?;
            let field_offset = size;
            size = size.checked_add(scalar.size)?;
            alignment = alignment.max(scalar.alignment);
            if matches!(
                shape,
                AggregateFieldShape::Nominal(_)
                    | AggregateFieldShape::ManagedReference(_)
                    | AggregateFieldShape::Scalar(SemanticTypeId::STRING)
            ) {
                pointer_map_offsets.push(field_offset);
            }
            fields.push(AggregateStaticField { abi_type, field_offset });
        }
        size = align_to(size, alignment.max(8))?;
        for member in child_nodes(self.database(), declaration).ok().flatten()?.iter().copied() {
            if node_kind(self.database(), member).ok().flatten() != Some(IndexedNodeKind::Field) {
                continue;
            }
            if let Some(injection) = composition_injection_field(self.database(), member).ok()? {
                if injection.owner_type != declaration || self.target().pointer_width != 64 {
                    return None;
                }
                let offset = size.checked_add(u64::from(injection.ordinal).checked_mul(8)?)?;
                pointer_map_offsets.push(offset);
                injected_fields.push((member, offset));
            }
        }
        size = size.checked_add(u64::try_from(injected_fields.len()).ok()?.checked_mul(8)?)?;
        // Event fields have dedicated physical slots but remain absent from the logical
        // value-field projection layout. Their slots are appended after all value fields,
        // and the managed allocator's object-zeroing makes each initially null.
        for member in child_nodes(self.database(), declaration).ok().flatten()?.iter().copied() {
            if node_kind(self.database(), member).ok().flatten() != Some(IndexedNodeKind::Field) {
                continue;
            }
            if let Some(event) = event_field_layout(self.database(), member).ok()? {
                if event.owner_type != declaration || self.target().pointer_width != 64 {
                    return None;
                }
                let offset = u64::from(event.slot_offset);
                size = size.max(offset.checked_add(8)?);
                alignment = alignment.max(8);
                pointer_map_offsets.push(offset);
            }
        }
        let object_size = align_to(size, alignment)?;
        Some(AggregateObjectLayout {
            declaration,
            object_size,
            object_alignment: alignment,
            pointer_map_offsets: pointer_map_offsets.into(),
            fields: fields.into(),
            injected_fields: injected_fields.into(),
        })
    }

    pub fn aggregate_static_plan(&self, literal: AstNodeKey) -> Option<AggregateStaticPlan> {
        self.aggregate_static_plan_for_specialization(literal, None)
    }

    fn dynamic_descriptor_getter(
        &self,
        literal: AstNodeKey,
        declaration: AstNodeKey,
        specialization: Option<&GenericSpecializationInstance>,
    ) -> Option<String> {
        let unit =
            self.typed_program().assembly.units.iter().find(|unit| {
                beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == declaration.unit
            })?;
        // Pack<T>'s concrete ordinary source constructor owns this descriptor.
        // Its getter is image-local, fully signature-qualified, and never aliases
        // a primitive provider box merely because the physical fields happen to match.
        if let Some(instance) = specialization {
            if beskid_queries::registered_declaration_name(self.database(), declaration)?.as_str() == "DynamicPackBoxV1"
                && literal.unit == instance.declaration.unit
                && declaration.unit == instance.declaration.unit
                && literal.generation == self.typed_program().generation
            {
                if let Some(shape) = self.compiled_dynamic_packing_shape(instance).ok().flatten() {
                    let digest = shape.sha256().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
                    return Some(format!("beskid_dynamic_pack_v1_{digest}_descriptor"));
                }
            }
        }
        if unit.logical_name != "src/Runtime/Dynamic/Records.bd"
            || literal.generation != self.typed_program().generation
            || !self.runtime_intrinsic_capability()?.authorizes_source(&unit.logical_name)
        {
            return None;
        }
        let declaration_name = beskid_queries::registered_declaration_name(self.database(), declaration)?;
        if declaration_name.as_str() == "DynamicErasedCellV1" {
            let layout = self.aggregate_object_layout(declaration)?;
            if layout.object_size != 40
                || layout.object_alignment != 8
                || layout.pointer_map_offsets.as_ref() != [16, 24]
                || layout.fields.iter().map(|field| field.field_offset).collect::<Vec<_>>() != [16, 24, 32]
                || layout.fields[2].abi_type != SemanticTypeId::U64
            {
                return None;
            }
            return Some("beskid_dynamic_v1_erased_cell_descriptor".into());
        }
        let role = match declaration_name.as_str() {
            "DynamicBoxV1" => "box",
            "DynamicCellV1" => "cell",
            _ => return None,
        };
        let substitution = specialization?.substitutions.iter().find(|binding| binding.parameter.as_ref() == "T")?;
        let label = if substitution.is_byte_array() {
            "bytes"
        } else {
            match substitution.exact_scalar_source()? {
                SemanticTypeId::I8 => "i8",
                SemanticTypeId::I16 => "i16",
                SemanticTypeId::I32 => "i32",
                SemanticTypeId::I64 => "i64",
                SemanticTypeId::U8 => "u8",
                SemanticTypeId::U16 => "u16",
                SemanticTypeId::U32 => "u32",
                SemanticTypeId::U64 => "u64",
                SemanticTypeId::F32 => "f32",
                SemanticTypeId::F64 => "f64",
                SemanticTypeId::BOOL => "bool",
                SemanticTypeId::CHAR => "char",
                SemanticTypeId::WORD => "word",
                SemanticTypeId::STRING => "string",
                SemanticTypeId::UNIT => "unit",
                _ => return None,
            }
        };
        Some(format!("beskid_rt_v5_dynamic_{label}_{role}_descriptor"))
    }

    pub fn aggregate_static_plan_for_specialization(
        &self,
        literal: AstNodeKey,
        specialization: Option<&GenericSpecializationInstance>,
    ) -> Option<AggregateStaticPlan> {
        let declaration = aggregate_literal_declaration(self.database(), literal).ok().flatten()?;
        let descriptor = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidTypeDescriptor")?;
        let request = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAllocationRequest")?;
        if descriptor.size != 40 || descriptor.alignment != 8 || request.size != 24 || request.alignment != 8 {
            return None;
        }
        let specialized = specialization.and_then(|specialization| {
            aggregate_literal_specialization(self.database(), literal, specialization.substitutions.clone())
                .ok()
                .flatten()
        });
        let aggregate =
            specialized.clone().or_else(|| aggregate_literal_layout(self.database(), literal).ok().flatten())?;
        let layout = self.aggregate_object_layout_from_fact(declaration, &aggregate)?;
        let unit =
            self.typed_program().assembly.units.iter().position(|unit| {
                beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == literal.unit
            })?;
        let specialization_identity = specialization
            .filter(|_| specialized.is_some())
            .map(generic_specialization_identity)
            .map(|identity| identity.iter().map(u32::to_string).collect::<Vec<_>>().join("_"));
        let identity = format!(
            "{}_u{unit}_g{}_n{}{}",
            artifact_namespace(self),
            literal.generation.0,
            literal.node.0,
            specialization_identity.as_deref().map(|identity| format!("_s{identity}")).unwrap_or_default()
        );
        let source = &self.typed_program().assembly.units[unit].logical_name;
        let utf8_record = source == "src/Runtime/Data/Utf8ViewRecord.bd"
            && self.runtime_intrinsic_capability()?.authorizes_source(source);
        let descriptor_flags = if utf8_record {
            let projection = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidUtf8ViewRecord")?;
            if projection.size != 40
                || projection.alignment != 8
                || projection.fields.iter().map(|field| field.offset).collect::<Vec<_>>() != [0, 8, 16, 24, 32]
            {
                return None;
            }
            if literal.generation != self.typed_program().generation
                || layout.object_size != 40
                || layout.object_alignment != 8
                || layout.pointer_map_offsets.as_ref() != [16]
                || layout.fields.len() != 3
                || layout.fields.iter().map(|field| field.field_offset).collect::<Vec<_>>() != [16, 24, 32]
                || layout.fields[1].abi_type != SemanticTypeId::POINTER
                || layout.fields[2].abi_type != SemanticTypeId::WORD
            {
                return None;
            }
            2
        } else {
            0
        };
        Some(AggregateStaticPlan {
            descriptor_flags,
            descriptor_getter: self.dynamic_descriptor_getter(literal, declaration, specialization),
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
        self.enum_static_plan_for_specialization(literal, None)
    }

    pub fn enum_static_plan_for_specialization(
        &self,
        literal: AstNodeKey,
        specialization: Option<&GenericSpecializationInstance>,
    ) -> Option<AggregateStaticPlan> {
        let specialized = specialization.and_then(|specialization| {
            enum_constructor_specialization(self.database(), literal, specialization.substitutions.clone())
                .ok()
                .flatten()
        });
        let propagated = beskid_queries::try_expression_fact(self.database(), literal).ok().flatten();
        let layout = propagated
            .map(|fact| fact.return_layout)
            .or_else(|| specialized.as_ref().map(|fact| fact.layout.clone()))
            .or_else(|| enum_layout(self.database(), literal).ok().flatten())
            .or_else(|| enum_match(self.database(), literal).ok().flatten().map(|fact| fact.layout))?;
        let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        let physical =
            layout.scalar_payload_object_layout(self.target().pointer_width, header.size, header.alignment)?;
        let fields =
            std::iter::once(AggregateStaticField { abi_type: SemanticTypeId::I32, field_offset: physical.tag_offset })
                .chain(physical.storage_fields.iter().map(|(abi_type, field_offset)| AggregateStaticField {
                    abi_type: *abi_type,
                    field_offset: *field_offset,
                }))
                .collect::<Vec<_>>();
        let unit =
            self.typed_program().assembly.units.iter().position(|unit| {
                beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == literal.unit
            })?;
        let specialization_identity = specialization
            .filter(|_| specialized.is_some())
            .map(generic_specialization_identity)
            .map(|identity| identity.iter().map(u32::to_string).collect::<Vec<_>>().join("_"));
        let identity = format!(
            "{}_enum_u{unit}_g{}_n{}{}",
            artifact_namespace(self),
            literal.generation.0,
            literal.node.0,
            specialization_identity.as_deref().map(|identity| format!("_s{identity}")).unwrap_or_default()
        );
        Some(AggregateStaticPlan {
            descriptor_flags: 0,
            descriptor_getter: None,
            literal,
            descriptor_symbol: format!("__beskid_aggregate_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_aggregate_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_aggregate_allocation_request_{identity}"),
            object_size: physical.object_size,
            object_alignment: physical.object_alignment,
            pointer_map_offsets: physical.pointer_map_offsets,
            fields: fields.into(),
        })
    }
}

fn artifact_namespace(input: &CodegenInput<'_>) -> String {
    input
        .artifact_namespace()
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect()
}

fn valid_alignment(value: u64) -> bool {
    value.is_power_of_two() && value > 0
}
fn align_to(value: u64, alignment: u64) -> Option<u64> {
    valid_alignment(alignment).then_some(())?;
    value.checked_add(alignment - 1).map(|value| value & !(alignment - 1))
}
