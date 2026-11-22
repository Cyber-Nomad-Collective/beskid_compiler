use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use beskid_abi::runtime_source::{
    OBSERVER_NODE_CLOSURE_OFFSET, OBSERVER_NODE_DESCRIPTOR_SYMBOL, OBSERVER_NODE_NEXT_OFFSET, OBSERVER_NODE_SIZE,
};
use beskid_isle::AstNodeKey;
use beskid_queries::{
    SourceNominalType, SourceSignatureType, child_nodes, closure_call_target, event_handler_lambda_for_local,
    source_aggregate_literal_type, source_record_fields, spawn_entry_validation,
};
use cranelift_codegen::ir::Endianness;
use cranelift_module::{DataDescription, DataId, Linkage, Module, ModuleResult};

use super::{
    contracts::{SyntaxModuleEmissionError, emission_verification},
    items::ResolvedSyntaxModuleItem,
    trampolines::{LambdaTrampoline, SpawnTrampoline},
};
use crate::{
    CodegenArtifact, CodegenInput,
    aggregate_static::{AggregateStaticPlan, emit_aggregate_static_data},
    array_static::{ArrayStaticPlan, emit_array_static_data},
    closure_static::{ClosureStaticPlan, emit_closure_static_data},
};

pub(super) fn collect_array_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    retain_source_types: bool,
) -> (Vec<ArrayStaticPlan>, HashMap<String, SourceSignatureType>) {
    let mut symbols = HashSet::new();
    let mut plans = Vec::new();
    let mut source_elements = HashMap::new();
    for item in items {
        let mut visited = HashSet::new();
        let mut nodes = Vec::new();
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
        for key in nodes {
            let plan = input
                .array_static_plan_for_specialization(key, item.specialization.as_ref())
                .or_else(|| input.bulk_array_static_plan(key, item.specialization.as_ref()))
                .or_else(|| input.typed_array_static_plan(key, item.specialization.as_ref()))
                .or_else(|| input.canonical_empty_statics(key, item.specialization.as_ref())?.array);
            if let Some(plan) = plan
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                if retain_source_types
                    && plan.element_type == beskid_queries::SemanticTypeId::POINTER
                    && let Ok(Some(element)) =
                        beskid_queries::source_array_element_type(input.database(), key, item.specialization.as_ref())
                {
                    source_elements.insert(plan.allocation_request_symbol.clone(), element);
                }
                plans.push(plan);
            }
        }
    }
    if let Some((composition, _)) = input.composition_authority() {
        for plural in &composition.plurals {
            let field = input.composition_unit_key(&plural.field_unit, plural.field_node_id);
            if let Some(plan) = input.composition_plural_static_plan(field, plural.target_slots.len())
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                if retain_source_types
                    && let Ok(Some(SourceSignatureType::Array(element))) =
                        beskid_queries::source_injected_field_type(input.database(), field)
                {
                    source_elements.insert(plan.allocation_request_symbol.clone(), *element);
                }
                plans.push(plan);
            }
        }
    }
    (plans, source_elements)
}

#[derive(Default)]
pub(super) struct AggregateSourceTypes {
    pub contracts: HashSet<SourceNominalType>,
    pub referenced_layouts: Vec<(SourceNominalType, crate::aggregate_static::AggregateObjectLayout)>,
    pub declarations: HashMap<String, SourceNominalType>,
    pub fields: HashMap<SourceNominalType, Arc<[SourceSignatureType]>>,
}

pub(super) fn collect_aggregate_static_plans<'a>(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    retain_source_types: bool,
    signature_and_element_types: impl Iterator<Item = &'a SourceSignatureType>,
) -> Result<(Vec<AggregateStaticPlan>, AggregateSourceTypes), SyntaxModuleEmissionError> {
    let mut symbols = HashSet::new();
    let mut plans = Vec::new();
    let mut source_types = AggregateSourceTypes::default();
    for item in items {
        let mut visited = HashSet::new();
        let mut nodes = Vec::new();
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
        for key in nodes {
            let plan = input
                .aggregate_static_plan_for_specialization(key, item.specialization.as_ref())
                .map(|plan| (plan, false))
                .or_else(|| {
                    input
                        .enum_static_plan_for_specialization(key, item.specialization.as_ref())
                        .map(|plan| (plan, true))
                })
                .or_else(|| input.canonical_empty_statics(key, item.specialization.as_ref())?.object.map(|plan| (plan, false)));
            if let Some((plan, is_enum)) = plan
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                if retain_source_types {
                    let db = input.database();
                    let shape = source_aggregate_literal_type(db, key, item.specialization.as_ref())
                        .ok()
                        .flatten()
                        .and_then(|nominal| {
                            (if is_enum {
                                super::enum_storage::enum_fields(input, &plan, &nominal, item.specialization.as_ref())
                            } else {
                                source_record_fields(db, &nominal).ok().flatten()
                            })
                            .map(|fields| (nominal, fields))
                        });
                    if let Some((nominal, fields)) = shape {
                        source_types.fields.insert(nominal.clone(), fields);
                        source_types.declarations.insert(plan.allocation_request_symbol.clone(), nominal);
                    }
                }
                plans.push(plan);
            }
        }
    }
    if let Some((composition, _)) = input.composition_authority() {
        for activation in &composition.activation {
            if let Some(mut plan) = input.composition_registration_static_plan(activation.registration_id)
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                if retain_source_types {
                    super::composition_source::retain_registration(input, &mut plan, &mut source_types)?;
                }
                plans.push(plan);
            }
        }
    }
    let db = input.database();
    for instance in items.iter().filter_map(|item| item.specialization.as_ref()) {
        let shape = beskid_queries::dynamic_shape_binding(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?;
        let unpack = beskid_queries::dynamic_unpacking_bridge(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?;
        let mapping = beskid_queries::dynamic_mapping_binding(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?;
        let packing = shape.as_ref().map(|binding| binding.packing())
            .or_else(|| unpack.as_ref().map(|bridge| bridge.packing()));
        let mapped = mapping.as_ref().into_iter().flat_map(|binding| [binding.source(), binding.destination()]);
        for packing in packing.into_iter().chain(mapped) {
            let plan = input.dynamic_mapping_box(packing).map_err(emission_verification)?;
            if retain_source_types && !source_types.declarations.contains_key(&plan.allocation_request_symbol) {
                let nominal = source_aggregate_literal_type(db, plan.literal, Some(packing))
                    .map_err(|error| emission_verification(error.to_string()))?
                    .ok_or_else(|| emission_verification("Dynamic Pack box lacks source type"))?;
                let fields = source_record_fields(db, &nominal)
                    .map_err(|error| emission_verification(error.to_string()))?
                    .ok_or_else(|| emission_verification("Dynamic Pack box lacks source fields"))?;
                source_types.fields.insert(nominal.clone(), fields);
                source_types.declarations.insert(plan.allocation_request_symbol.clone(), nominal);
            }
            if symbols.insert(plan.allocation_request_symbol.clone()) {
                plans.push(plan);
            } else if !plans.contains(&plan) {
                return Err(emission_verification("Dynamic Pack box allocation conflict"));
            }
        }
    }
    if retain_source_types {
        // Follow declaration order from encountered plans, preserving stable Wasm type indices.
        let mut pending = signature_and_element_types.cloned().collect::<Vec<_>>();
        for plan in &plans {
            if let Some(nominal) = source_types.declarations.get(&plan.allocation_request_symbol)
                && let Some(fields) = source_types.fields.get(nominal)
            {
                pending.extend(fields.iter().cloned());
            }
        }
        let mut seen = HashSet::new();
        while let Some(field) = pending.pop() {
            match field {
                SourceSignatureType::Nominal(nominal) if seen.insert(nominal.clone()) => {
                    if matches!(beskid_queries::node_kind(input.database(), nominal.declaration),
                        Ok(Some(beskid_queries::IndexedNodeKind::ContractDefinition))) {
                        source_types.contracts.insert(nominal);
                        continue;
                    }
                    if source_types.fields.contains_key(&nominal) { continue; }
                    // GC enum storage reserves the tag followed by disjoint variant payloads.
                    // A type may only appear in an inactive variant and have no constructor in this module.
                    let fields = source_record_fields(input.database(), &nominal).ok().flatten().or_else(|| {
                        let variants = beskid_queries::source_enum_variant_fields(input.database(), &nominal).ok()??;
                        let tag = SourceSignatureType::Scalar(beskid_queries::SemanticTypeId::I32);
                        let unit = SourceSignatureType::Scalar(beskid_queries::SemanticTypeId::UNIT);
                        Some(std::iter::once(tag).chain(variants.into_iter().flatten().filter(|ty| ty != &unit)).collect())
                    });
                    if let Some(fields) = fields
                        && let Some(layout) = input.aggregate_object_layout_from_types(nominal.declaration, input.observed_properties.has_source_slot(input.database(), &nominal), fields.iter().map(|field| match field {
                            SourceSignatureType::Scalar(ty) => (*ty, *ty == beskid_queries::SemanticTypeId::STRING),
                            _ => (beskid_queries::SemanticTypeId::POINTER, true),
                        }))
                    {
                        pending.extend(fields.iter().cloned());
                        source_types.fields.insert(nominal.clone(), fields);
                        source_types.referenced_layouts.push((nominal, layout));
                    }
                }
                SourceSignatureType::Array(element) => pending.push(*element),
                SourceSignatureType::Function { parameters, result } => {
                    pending.extend(parameters.iter().cloned());
                    pending.push(*result);
                }
                _ => {}
            }
        }
    }
    Ok((plans, source_types))
}

pub(super) type CaptureTypes = HashMap<String, Vec<Option<SourceSignatureType>>>;

/// Collect source-proven closure static plans from generation-safe syntax facts.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
pub(super) fn collect_closure_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    trampolines: &[SpawnTrampoline],
    lambda_trampolines: &[LambdaTrampoline],
    retain_source_types: bool,
) -> (Vec<ClosureStaticPlan>, HashMap<String, String>, CaptureTypes) {
    let db = input.database();
    let mut plans = Vec::new();
    let mut environments = HashMap::new();
    let mut capture_types = HashMap::new();
    let mut seen = HashSet::new();
    // One lambda of a generic body has one plan per enclosing instance, told apart by symbol.
    let mut push_plan = |plan: ClosureStaticPlan, entry: Option<&str>, enclosing: Option<&beskid_queries::GenericSpecializationInstance>| {
        if retain_source_types && let Some(entry) = entry {
            environments.insert(entry.to_owned(), plan.allocation_request_symbol.clone());
        }
        if seen.insert(plan.descriptor_symbol.clone()) {
            if retain_source_types {
                let fields = plan.captures.iter().map(|field| {
                    beskid_queries::source_capture_type(db, field.capture.declaration, enclosing).ok().flatten()
                }).collect();
                capture_types.insert(plan.allocation_request_symbol.clone(), fields);
            }
            plans.push(plan);
        }
    };
    for trampoline in trampolines {
        if trampoline.closure_captures.is_some()
            && let Ok(Some(validation)) = spawn_entry_validation(db, trampoline.spawn)
            && let Some(authority) = input.closure_lowering_authority(trampoline.spawn, validation.target)
        {
            push_plan(authority.plan, Some(&trampoline.target_symbol), None);
        }
    }
    for trampoline in lambda_trampolines {
        if trampoline.closure_captures.is_some()
            && let Some(instance) = trampoline.specialization.as_ref()
        {
            if let Some(authority) =
                input.specialized_closure_lowering_authority(trampoline.lambda, trampoline.lambda, instance)
            {
                push_plan(authority.plan, Some(&trampoline.symbol), Some(instance));
            }
            continue;
        }
        if trampoline.closure_captures.is_some() {
            let event_handler = event_handler_lambda_for_local(db, trampoline.lambda).ok().flatten().is_some();
            let authority = if event_handler {
                input.event_handler_closure_lowering_authority(trampoline.lambda, trampoline.lambda)
            } else {
                input.closure_lowering_authority(trampoline.lambda, trampoline.lambda)
            };
            if let Some(authority) = authority {
                push_plan(authority.plan, Some(&trampoline.symbol), None);
            }
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
            push_plan(authority.plan, None, None);
        }
    }
    (plans, environments, capture_types)
}

pub(super) fn event_handler_wrapper_required(input: &CodegenInput<'_>, items: &[ResolvedSyntaxModuleItem]) -> bool {
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
    }
    // Function values (materialized lambdas and named functions read as values) share the
    // event-handler closure record layout and allocation request.
    nodes.into_iter().any(|key| {
        matches!(
            beskid_queries::event_operation(input.database(), key),
            Ok(Some(fact)) if fact.handler_lambda.is_some()
        ) || matches!(beskid_queries::lambda_value_required(input.database(), key), Ok(Some(true)))
            || matches!(beskid_queries::function_item_value(input.database(), key), Ok(Some(_)))
    })
}

pub(super) fn collect_ast_nodes(
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

/// Declare the runtime kit data symbols that lowered functions read through a symbol global as
/// imports: today the write-barrier process word `beskid_rt_v5_gc_barrier_active`, which the
/// guarded barrier of a managed field or element store loads. The kit defines it; a module that
/// does not read it declares nothing.
pub(crate) fn declare_runtime_data_imports<M: Module>(module: &mut M, artifact: &CodegenArtifact) -> ModuleResult<()> {
    for function in &artifact.functions {
        declare_function_runtime_data_imports(module, &function.function)?;
    }
    Ok(())
}

/// Declare the runtime kit data imports one lowered function reads ([`declare_runtime_data_imports`]),
/// for bodies that are defined outside the artifact (checked-effect closures) before their symbol
/// globals are remapped. Declaring the same import twice yields the same data id.
pub(crate) fn declare_function_runtime_data_imports<M: Module>(
    module: &mut M,
    function: &cranelift_codegen::ir::Function,
) -> ModuleResult<()> {
    for global in function.global_values.values() {
        if let cranelift_codegen::ir::GlobalValueData::Symbol {
            name: cranelift_codegen::ir::ExternalName::TestCase(name),
            ..
        } = global
        {
            let name = name.raw();
            if name == beskid_isle::GC_BARRIER_ACTIVE_SYMBOL.as_bytes() {
                module.declare_data(beskid_isle::GC_BARRIER_ACTIVE_SYMBOL, Linkage::Import, false, false)?;
            } else if name == OBSERVER_NODE_DESCRIPTOR_SYMBOL.as_bytes()
                && module.get_name(OBSERVER_NODE_DESCRIPTOR_SYMBOL).is_none()
            {
                emit_observer_node_descriptor(module)?;
            }
        }
    }
    Ok(())
}

fn emit_observer_node_descriptor<M: Module>(module: &mut M) -> ModuleResult<()> {
    let pointer_map = module.declare_data("__beskid_observer_node_pointer_map_v5", Linkage::Local, false, false)?;
    let descriptor = module.declare_data(OBSERVER_NODE_DESCRIPTOR_SYMBOL, Linkage::Local, false, false)?;
    let mut pointer_map_data = DataDescription::new();
    let pointer_offsets = [OBSERVER_NODE_CLOSURE_OFFSET as u64, OBSERVER_NODE_NEXT_OFFSET as u64];
    let pointer_bytes = pointer_offsets.into_iter().flat_map(u64::to_le_bytes).collect::<Vec<_>>();
    pointer_map_data.define(pointer_bytes.into_boxed_slice());
    pointer_map_data.set_align(8);
    module.define_data(pointer_map, &pointer_map_data)?;

    // ABI-v5 BeskidTypeDescriptor has the GC pointer-map address at offset 16.
    let descriptor_words = [OBSERVER_NODE_SIZE, 8, 0, pointer_offsets.len() as u64, 0];
    let descriptor_bytes = descriptor_words.into_iter().flat_map(u64::to_le_bytes).collect::<Vec<_>>();
    let mut descriptor_data = DataDescription::new();
    descriptor_data.define(descriptor_bytes.into_boxed_slice());
    descriptor_data.set_align(8);
    let pointer_map_address = module.declare_data_in_data(pointer_map, &mut descriptor_data);
    descriptor_data.write_data_addr(16, pointer_map_address, 0);
    module.define_data(descriptor, &descriptor_data)
}

/// Emit artifact-owned closure descriptor/pointer-map/allocation-request data, after declaring the
/// runtime data symbols the artifact's functions import ([`declare_runtime_data_imports`]).
pub fn emit_closure_static_plans<M: Module>(module: &mut M, artifact: &CodegenArtifact) -> ModuleResult<()> {
    declare_runtime_data_imports(module, artifact)?;
    if artifact.event_handler_wrapper_required {
        crate::emit_event_handler_static_data(module)?;
    }
    for plan in &artifact.closure_static_plans {
        emit_closure_static_data(module, plan)?;
    }
    for plan in &artifact.aggregate_static_plans {
        emit_aggregate_static_data(module, plan)?;
    }
    for plan in &artifact.array_static_plans {
        emit_array_static_data(module, plan)?;
    }
    if artifact.contract_boxes_required {
        crate::contract_static::emit_contract_box_static_data(module)?;
    }
    Ok(())
}

/// Define one module-local data object per entry in `artifact.string_literals`.
pub fn emit_string_literals<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
) -> ModuleResult<HashMap<String, DataId>> {
    let mut handles = HashMap::new();
    // Declare in symbol order so the object layout does not follow hash-map iteration order.
    let mut literals = artifact.string_literals.iter().collect::<Vec<_>>();
    literals.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (symbol, data) in literals {
        let data_id = module.declare_data(symbol, Linkage::Local, false, false)?;
        let mut ctx = DataDescription::new();
        ctx.define(data.clone().into_boxed_slice());
        module.define_data(data_id, &ctx)?;
        handles.insert(symbol.clone(), data_id);
    }
    Ok(handles)
}
