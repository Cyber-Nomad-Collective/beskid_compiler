//! Replace the native memory representation of the probe's record and array.
//! Scalar instructions and control flow remain in the original CLIF graph.
#![allow(non_snake_case)]

mod array_requests;
mod spills;
mod transfers;
mod contracts;
mod event_calls;
pub(crate) mod dynamic;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
};

use crate::browser_heap::{observers, ArrayElement, ArrayOp, Field, HeapLayout, RecordOp, Scalar};
use crate::strings::{self, StringPool};
use crate::{math, network, suspension, text_services};
use beskid_abi::{
    runtime_source::{OBSERVER_NODE_CLOSURE_OFFSET, OBSERVER_NODE_DESCRIPTOR_SYMBOL, OBSERVER_NODE_NEXT_OFFSET, OBSERVER_NODE_PROPERTY_OFFSET},
    SYM_ARRAY_LEN, SYM_STR_CONCAT, SYM_STR_EQ, SYM_STR_FROM_BYTES_UTF8, SYM_STR_FROM_I64, SYM_STR_LEN, SYM_STR_NEW,
    SYM_STR_SLICE,
};
use beskid_codegen::artifact::CodegenArtifact;
use beskid_codegen::{CallableSite, GC_BARRIER_ACTIVE_SYMBOL, GC_WRITE_BARRIER_SYMBOL};
use beskid_queries::{SemanticTypeId, SourceSignatureType};
use cranelift_codegen::{
    cursor::{Cursor, FuncCursor},
    dominator_tree::DominatorTree,
    flowgraph::ControlFlowGraph,
    ir::{
        AbiParam, Block, ExtFuncData, ExternalName, FuncRef, Function, GlobalValue, GlobalValueData, Inst, InstBuilder,
        InstructionData, Opcode, Signature, StackSlot, Value, ValueDef,
        types::{I8, I32, I64},
    },
    settings,
    verifier::verify_function,
};

const SAME_IDENTITY: &str = "beskid_rt_v5_gc_same_identity";
const ARRAY_ALLOC: &str = "beskid_rt_v5_array_allocate";
const ARRAY_WRITE_BARRIER: &str = "beskid_rt_v5_array_write_barrier";
const ARRAY_TRY_GROW: &str = "beskid_rt_v5_array_try_grow";
const ARRAY_GROW: &str = "beskid_rt_v5_array_grow";
const PACKET_ALLOC: &str = "beskid_rt_v5_managed_object_allocate";
const CAPTURE_STORE: &str = beskid_codegen::closure_static::ABI_V5_CLOSURE_CAPTURE_STORE;
const CLOSURE_ALLOC: &str = beskid_codegen::closure_static::ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE;
const POLL: &str = beskid_abi::SYM_GC_COLLECT_IF_NEEDED;

pub fn adapt(artifact: &mut CodegenArtifact) -> Result<HeapLayout, Box<dyn Error>> {
    let strings = strings::collect(artifact)?;
    adapt_with_strings(artifact, &strings)
}

pub(crate) fn adapt_with_strings(
    artifact: &mut CodegenArtifact,
    strings: &StringPool,
) -> Result<HeapLayout, Box<dyn Error>> {
    let arrays = &artifact.array_static_plans;
    // Shared lowering also retains layouts consulted for pattern matching. Only an actual
    // allocation request needs a GC type in the linked Wasm program.
    let referenced_descriptors: HashSet<&[u8]> = artifact
        .functions
        .iter()
        .flat_map(|lowered| {
            lowered.function.global_values.values().filter_map(|data| match data {
                GlobalValueData::Symbol { name: ExternalName::TestCase(name), .. } => Some(name.raw()),
                _ => None,
            })
        })
        .collect();
    let packets: Vec<_> = artifact
        .aggregate_static_plans
        .iter()
        .filter(|plan| referenced_descriptors.contains(plan.allocation_request_symbol.as_bytes()) || dynamic::requires_box(artifact, plan))
        .collect();
    let userNames: HashSet<String> = artifact.functions.iter().map(|f| f.name.clone()).collect();
    let nativeSignatures: HashMap<String, Signature> =
        artifact.functions.iter().map(|f| (f.name.clone(), f.function.signature.clone())).collect();
    // Assign one layout per source type before projecting reference-valued fields.
    let mut nominalLayouts = HashMap::new();
    let mut canonicalPackets = Vec::new();
    for &packet in &packets {
        if artifact.spawn_artifacts.iter().any(|spawn| spawn.result_request == packet.allocation_request_symbol || spawn.argument_request.as_ref() == Some(&packet.allocation_request_symbol)) { continue; }
        let declaration = artifact
            .aggregate_declarations
            .get(&packet.allocation_request_symbol)
            .ok_or_else(|| format!("record allocation {} has no proven source declaration at {:?}", packet.allocation_request_symbol, packet.literal))?;
        if let Some(&previous) = nominalLayouts.get(declaration) {
            let (_, fields, pointers, observer_slot): &(_, &[_], &[_], Option<u32>) = &canonicalPackets[previous];
            if *fields != packet.fields.as_ref() || *pointers != packet.pointer_map_offsets.as_ref() || *observer_slot != packet.observer_slot {
                return Err("nominal type has ambiguous record layouts".into());
            }
        } else {
            nominalLayouts.insert(declaration, canonicalPackets.len());
            canonicalPackets.push((declaration, packet.fields.as_ref(), packet.pointer_map_offsets.as_ref(), packet.observer_slot));
        }
    }
    for (nominal, layout) in &artifact.referenced_record_layouts {
        if !nominalLayouts.contains_key(nominal) {
            nominalLayouts.insert(nominal, canonicalPackets.len());
            canonicalPackets.push((nominal, layout.fields.as_ref(), layout.pointer_map_offsets.as_ref(), layout.observer_slot));
        }
    }
    let contractRecord = (artifact.contract_boxes_required || !artifact.source_contracts.is_empty()).then_some(canonicalPackets.len());
    if let Some(record) = contractRecord {
        for nominal in &artifact.source_contracts {
            nominalLayouts.insert(nominal, record);
        }
    }
    let wrapperRecord = (artifact.event_handler_wrapper_required || !artifact.browser_event_layouts.is_empty())
        .then_some(canonicalPackets.len() + usize::from(contractRecord.is_some()));
    let sourceElement = |mut source: &SourceSignatureType| {
        let mut depth = 0;
        while let SourceSignatureType::Array(inner) = source {
            depth += 1;
            source = inner;
        }
        let leaf = match source {
            SourceSignatureType::Scalar(ty) => array_element(*ty),
            SourceSignatureType::Nominal(nominal) => nominalLayouts.get(nominal).copied().map(ArrayElement::Record),
            SourceSignatureType::Function { .. } => wrapperRecord.map(ArrayElement::Record),
            _ => None,
        }?;
        (0..depth).try_fold(leaf, |element, _| element.nested())
    };
    let mut elements = std::collections::BTreeSet::new();
    let mut allocations = Vec::new();
    for array in arrays {
        let kind = if array.element_type == SemanticTypeId::POINTER {
            artifact.array_source_elements.get(&array.allocation_request_symbol).and_then(&sourceElement)
        } else {
            array_element(array.element_type)
        }
        .ok_or_else(|| {
            format!("array {} has no proven representation for element {:?} (source {:?})", array.allocation_request_symbol, array.element_type, artifact.array_source_elements.get(&array.allocation_request_symbol))
        })?;
        let expectedPointers: &[u64] = if matches!(kind, ArrayElement::Scalar(_)) { &[] } else { &[0] };
        if array.pointer_map_offsets.as_ref() != expectedPointers {
            return Err("array element pointer map disagrees with its type".into());
        }
        if u64::from(native_element_type(kind).bytes()) != array.stride {
            return Err("unsupported array element stride".into());
        }
        elements.insert(kind);
        allocations.push((u32::try_from(array.length)? as i32, kind));
    }
    let mut records = Vec::new();
    let mut offsets = Vec::new();
    let mut observedRecords = Vec::new();
    let mut events = Vec::new();
    for (declaration, physical_fields, _, observer_slot) in canonicalPackets {
        let sourceFields = artifact
            .aggregate_source_fields
            .get(declaration)
            .ok_or("record declaration has no source field identities")?;
        if sourceFields.len() != physical_fields.len() {
            return Err("record source fields and ABI layout disagree".into());
        }
        let mut fields = physical_fields
            .iter()
            .zip(sourceFields.iter())
            .map(|(field, source)| match source {
                SourceSignatureType::Array(item) if field.abi_type == SemanticTypeId::POINTER => {
                    sourceElement(item).map(Field::Array).ok_or("unsupported array field element")
                }
                SourceSignatureType::Scalar(SemanticTypeId::STRING)
                    if matches!(field.abi_type, SemanticTypeId::POINTER | SemanticTypeId::STRING) =>
                {
                    Ok(Field::String)
                }
                SourceSignatureType::Function { .. } if field.abi_type == SemanticTypeId::POINTER => {
                    wrapperRecord.map(Field::Record).ok_or("function record field has no closure layout")
                }
                SourceSignatureType::Nominal(declaration) if field.abi_type == SemanticTypeId::POINTER => {
                    nominalLayouts
                        .get(declaration)
                        .copied()
                        .map(Field::Record)
                        .ok_or("nominal record field has no proven storage layout")
                }
                SourceSignatureType::Scalar(SemanticTypeId::UNIT) if field.abi_type == SemanticTypeId::UNIT => {
                    // Keep the source field index in GC storage. Native lowering allocates no bytes
                    // for unit and may give a following field the same byte offset.
                    Ok(Field::Scalar(Scalar::I8))
                }
                SourceSignatureType::Scalar(ty) if *ty == field.abi_type => {
                    scalar(*ty).map(Field::Scalar).ok_or("unsupported record scalar type")
                }
                _ => Err("record field lacks proven supported source type"),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut record_offsets = physical_fields.iter().map(|field| {
            if field.abi_type == SemanticTypeId::UNIT { -1 } else { field.field_offset as i64 }
        }).collect::<Vec<_>>();
        if let Some(layouts) = artifact.browser_event_layouts.get(&declaration.declaration) {
            let handler = wrapperRecord.ok_or("managed event handler layout unavailable")?;
            for event in layouts {
                if event.capacity == 0 || event.owner_type != declaration.declaration {
                    return Err("event layout disagrees with the checked owner".into());
                }
                events.push(crate::browser_heap::events::Event {
                    owner: records.len(),
                    field: fields.len(),
                    source_offset: event.slot_offset,
                    capacity: event.capacity,
                    handler,
                });
                fields.push(Field::Array(ArrayElement::Record(handler)));
                record_offsets.push(i64::from(event.slot_offset));
            }
        }
        if let Some(offset) = observer_slot { observedRecords.push((records.len(), offset)); }
        records.push(fields);
        offsets.push(record_offsets);
    }
    if contractRecord.is_some() {
        records.push(vec![Field::AnyRecord, Field::Scalar(Scalar::I64), Field::Scalar(Scalar::I64)]);
        offsets.push(vec![-1,16,24]);
    }
    if let Some(record) = wrapperRecord {
        debug_assert_eq!(record, records.len());
        records.push(vec![Field::Scalar(Scalar::I64), Field::AnyRecord]);
        offsets.push(vec![16, 24]);
    }
    let mut syntheticLayouts = HashMap::new();
    let mut fiberResultLayouts = HashMap::new();
    for spawn in &artifact.spawn_artifacts {
        let Some(plan) = packets.iter().find(|plan| plan.allocation_request_symbol == spawn.result_request) else { continue; };
        let source = spawn.handle_type.arguments.first().ok_or("fiber result source type missing")?;
        let [field] = plan.fields.as_ref() else { return Err("fiber result layout must contain one field".into()); };
        let pointer = plan.pointer_map_offsets.contains(&field.field_offset);
        let kind = match source {
            SourceSignatureType::Scalar(SemanticTypeId::STRING) if pointer => Field::String,
            SourceSignatureType::Scalar(SemanticTypeId::UNIT) if !pointer => Field::Scalar(Scalar::I64),
            SourceSignatureType::Scalar(ty) if !pointer && *ty == field.abi_type => Field::Scalar(scalar(*ty).ok_or("fiber result scalar unsupported")?),
            SourceSignatureType::Function { .. } if pointer => Field::Record(wrapperRecord.ok_or("fiber result closure layout missing")?),
            SourceSignatureType::Nominal(nominal) if pointer => Field::Record(*nominalLayouts.get(nominal).ok_or("fiber result record layout missing")?),
            SourceSignatureType::Array(item) if pointer => {
                let element = sourceElement(item).ok_or("fiber result array unsupported")?;
                elements.insert(element);
                Field::Array(element)
            }
            _ => return Err("fiber result source type disagrees with pointer map".into()),
        };
        let result_record = if let Some(&index) = fiberResultLayouts.get(source) {
            if records[index] != [kind] || offsets[index] != [field.field_offset as i64] {
                return Err("fiber result type has inconsistent layouts".into());
            }
            index
        } else {
            let index = records.len();
            records.push(vec![kind]);
            offsets.push(vec![field.field_offset as i64]);
            fiberResultLayouts.insert(source.clone(), index);
            index
        };
        syntheticLayouts.insert(spawn.result_request.clone(), result_record);
        if let Some(request) = &spawn.argument_request {
            let plan = packets.iter().find(|plan| &plan.allocation_request_symbol == request).ok_or("spawn argument layout missing")?;
            let signature = artifact.source_signatures.get(&spawn.target).ok_or("spawn target source signature missing")?;
            if plan.fields.len() != signature.parameters.len() { return Err("spawn argument layout and source signature disagree".into()); }
            let mut fields = Vec::new();
            for (field, source) in plan.fields.iter().zip(signature.parameters.iter()) {
                let pointer = plan.pointer_map_offsets.contains(&field.field_offset);
                let kind = match source {
                    SourceSignatureType::Scalar(SemanticTypeId::STRING) if pointer => Field::String,
                    SourceSignatureType::Scalar(ty) if !pointer && *ty == field.abi_type => Field::Scalar(scalar(*ty).ok_or("spawn argument scalar unsupported")?),
                    SourceSignatureType::Function { .. } if pointer => Field::Record(wrapperRecord.ok_or("spawn argument closure layout missing")?),
                    SourceSignatureType::Nominal(nominal) if pointer => Field::Record(*nominalLayouts.get(nominal).ok_or("spawn argument record layout missing")?),
                    SourceSignatureType::Array(item) if pointer => {
                        let element = sourceElement(item).ok_or("spawn argument array unsupported")?;
                        elements.insert(element);
                        Field::Array(element)
                    }
                    _ => return Err("spawn argument source type disagrees with pointer map".into()),
                };
                fields.push(kind);
            }
            syntheticLayouts.insert(request.clone(), records.len());
            offsets.push(plan.fields.iter().map(|field| field.field_offset as i64).collect());
            records.push(fields);
        }
    }
    let mut closureLayouts = Vec::new();
    let mut captureStores = HashMap::new();
    for plan in &artifact.closure_static_plans {
        if plan.captures.is_empty() {
            continue;
        }
        let captureTypes = artifact.closure_capture_types.get(&plan.allocation_request_symbol)
            .ok_or("closure environment has no source field types")?;
        if captureTypes.len() != plan.captures.len() { return Err("closure environment field count mismatch".into()); }
        let fields = plan.captures.iter().zip(captureTypes).map(|(field, source)| {
            let ty = source.as_ref().ok_or("closure capture has no proven source type")?;
            let is_pointer = plan.pointer_map_offsets.contains(&field.field_offset);
            if is_pointer != field.pointer_map_index.is_some() { return Err("closure capture pointer map disagrees with field"); }
            let kind = match ty {
                SourceSignatureType::Scalar(SemanticTypeId::STRING) if is_pointer => Field::String,
                SourceSignatureType::Scalar(ty) if !is_pointer && *ty == field.abi_type => Field::Scalar(scalar(*ty).ok_or("unsupported scalar closure capture")?),
                SourceSignatureType::Function { .. } if is_pointer => Field::Record(wrapperRecord.ok_or("closure capture closure layout missing")?),
                SourceSignatureType::Nominal(nominal) if is_pointer => Field::Record(*nominalLayouts.get(nominal).ok_or("closure capture record layout missing")?),
                SourceSignatureType::Array(item) if is_pointer => {
                    let element = sourceElement(item).ok_or("unsupported closure capture array element")?;
                    elements.insert(element);
                    Field::Array(element)
                }
                _ => return Err("unsupported or inconsistent closure capture type"),
            };
            Ok(kind)
        }).collect::<Result<Vec<_>, &str>>()?;
        let mut pointer_fields = Vec::new();
        for (index, field) in plan.captures.iter().enumerate() {
            if let Some(map_index) = field.pointer_map_index {
                if map_index as usize != pointer_fields.len() { return Err("non-contiguous closure capture pointer map".into()); }
                pointer_fields.push(index);
            }
        }
        if pointer_fields.len() != plan.pointer_map_offsets.len() { return Err("closure capture pointer map has unknown fields".into()); }
        captureStores.insert(records.len(), pointer_fields);
        closureLayouts.push((&plan.allocation_request_symbol, &plan.descriptor_symbol, records.len()));
        offsets.push(plan.captures.iter().map(|field| field.field_offset as i64).collect());
        records.push(fields);
    }
    let observers = if observedRecords.is_empty() {
        None
    } else {
        let callback = wrapperRecord.ok_or("observed properties have no closure record layout")?;
        let node = records.len();
        records.push(vec![Field::Record(callback), Field::Record(node), Field::Scalar(Scalar::I64)]);
        offsets.push(vec![
            i64::from(OBSERVER_NODE_CLOSURE_OFFSET),
            i64::from(OBSERVER_NODE_NEXT_OFFSET),
            i64::from(OBSERVER_NODE_PROPERTY_OFFSET),
        ]);
        let owners = observedRecords.into_iter().map(|(record, offset)| {
            let field = records[record].len() as u32 + 1;
            records[record].push(Field::Record(node));
            offsets[record].push(i64::from(offset));
            (record, field, offset)
        }).collect();
        Some(observers::Observers { node, callback, owners })
    };
    let project = |ty: &SourceSignatureType| -> Result<Option<ReferenceKind>, Box<dyn Error>> {
        match ty {
            SourceSignatureType::Scalar(SemanticTypeId::STRING) => Ok(Some(ReferenceKind::String)),
            SourceSignatureType::Scalar(ty) if *ty == SemanticTypeId::UNIT || scalar(*ty).is_some() => Ok(None),
            SourceSignatureType::Nominal(declaration) => nominalLayouts
                .get(declaration)
                .copied()
                .map(|index| Some(ReferenceKind::Record(index)))
                .ok_or_else(|| format!("no allocation layout for source type {declaration:?}").into()),
            SourceSignatureType::Function { .. } => wrapperRecord.map(|record| Some(ReferenceKind::Record(record))).ok_or_else(|| "function wrapper is unavailable".into()),
            SourceSignatureType::Array(item) => sourceElement(item)
                .map(|element| Some(ReferenceKind::Array(element)))
                .ok_or_else(|| "unsupported array signature element".into()),
            _ => Err("unsupported source signature type".into()),
        }
    };

    let project_callable = |source: &beskid_queries::SourceItemSignature| -> Result<CallableShape, Box<dyn Error>> {
        let unit = SourceSignatureType::Scalar(SemanticTypeId::UNIT);
        let value = |ty| Ok::<_, Box<dyn Error>>((raw_source_type(ty)?, project(ty)?));
        Ok(CallableShape {
            parameters: source.parameters.iter().filter(|ty| **ty != unit).map(value).collect::<Result<_, _>>()?,
            result: (&source.result != &unit).then(|| value(&source.result)).transpose()?,
        })
    };
    let closure_calls = artifact.closure_call_signatures.iter().filter(|(function, _)| userNames.contains(*function)).map(|(function, calls)| {
        let calls = calls.iter().map(|(&inst, source)| Ok((inst, project_callable(source)?)))
            .collect::<Result<HashMap<_, _>, Box<dyn Error>>>()?;
        Ok((function.as_str(), calls))
    }).collect::<Result<HashMap<_, _>, Box<dyn Error>>>()?;

    let contract_results = artifact.contract_call_results.iter().map(|(function, calls)| {
        let calls = calls.iter().map(|(&inst, ty)| Ok((inst, project(ty)?)))
            .collect::<Result<HashMap<_, _>, Box<dyn Error>>>()?;
        Ok((function.as_str(), calls))
    }).collect::<Result<HashMap<_, _>, Box<dyn Error>>>()?;
    let transfer_receivers = artifact.functions.iter().filter_map(|function| {
        let receives = function.function.dfg.ext_funcs.values().any(|external| {
            let ExternalName::TestCase(name) = &external.name else { return false; };
            std::str::from_utf8(name.raw()).ok().and_then(crate::scheduler::find)
                .is_some_and(|service| service.transfer == crate::scheduler::Transfer::Receive)
        });
        if !receives { return None; }
        let source = artifact.source_signatures.get(&function.name)?;
        let item = source.parameters.iter().find_map(|parameter| {
            let SourceSignatureType::Nominal(nominal) = parameter else { return None; };
            nominal.arguments.first()
        })?;
        Some((|| Ok::<_, Box<dyn Error>>((function.name.clone(), (raw_source_type(item)?, project(item)?))))())
    }).collect::<Result<HashMap<_, _>, _>>()?;
    let spawnEntries: HashMap<String, i64> = artifact.spawn_artifacts.iter().enumerate()
        .map(|(index, spawn)| (spawn.trampoline.clone(), index as i64 + 1)).collect();
    let closure_environment = |entry: &str| -> Result<Option<ReferenceKind>, &'static str> {
        artifact.closure_environment_requests.get(entry).map(|request| {
            closureLayouts.iter().find(|(candidate, _, _)| *candidate == request)
                .map(|(_, _, index)| ReferenceKind::Record(*index))
                .ok_or("closure environment has no proven allocation layout")
        }).transpose()
    };
    let mut signatures = HashMap::new();
    for lowered in &artifact.functions {
        if let Some(spawn) = artifact.spawn_artifacts.iter().find(|spawn| spawn.trampoline == lowered.name) {
            let record = *syntheticLayouts.get(&spawn.result_request).ok_or("spawn result layout unavailable")?;
            let signature = &lowered.function.signature;
            if signature.params != [AbiParam::new(I64)] || signature.returns != [AbiParam::new(I64)] {
                return Err("spawn trampoline has an unexpected ABI".into());
            }
            let environment = if let Some(request) = &spawn.argument_request {
                Some(ReferenceKind::Record(*syntheticLayouts.get(request).ok_or("spawn argument layout unavailable")?))
            } else { closure_environment(&spawn.target)? };
            signatures.insert(lowered.name.clone(), (vec![environment], Some(ReferenceKind::Record(record))));
            continue;
        }
        let source = artifact.source_signatures.get(&lowered.name)
            .ok_or_else(|| format!("function {} has no proven source signature", lowered.name))?;
        let shape = project_callable(source)?;
        let environment = closure_environment(&lowered.name)?;
        if !shape.matches(&lowered.function.signature, environment.is_some()) {
            return Err(format!("function {} source and CLIF signatures disagree: source {:?}, CLIF {}", lowered.name, source, lowered.function.signature).into());
        }
        let params = environment.map(Some).into_iter().chain(shape.parameters.into_iter().map(|(_, kind)| kind)).collect();
        signatures.insert(lowered.name.clone(), (params, shape.result.and_then(|(_, kind)| kind)));
    }

    let arrays_in_signatures = signatures.values().flat_map(|(params, result): &(Vec<_>, _)| params.iter().copied().chain(std::iter::once(*result)))
        .chain(closure_calls.values().flat_map(|calls| calls.values().flat_map(|shape| {
            shape.parameters.iter().map(|&(_, kind)| kind).chain(shape.result.map(|(_, kind)| kind))
        })));
    elements.extend(arrays_in_signatures.filter_map(|kind| match kind { Some(ReferenceKind::Array(element)) => Some(element), _ => None }));
    let mut dynamic = dynamic::DynamicProgram::prepare(artifact, &nominalLayouts, &mut records, &mut elements)?;
    elements.extend(transfer_receivers.values().filter_map(|(_, kind)| match kind {
        Some(ReferenceKind::Array(element)) => Some(*element), _ => None,
    }));
    let layout = HeapLayout { arrays: elements, records: records.clone(), observers, events };

    let mut fiberPayloads = HashMap::new();
    for spawn in &artifact.spawn_artifacts {
        let handle = *nominalLayouts.get(&spawn.handle_type).ok_or("fiber handle layout missing")?;
        let payload = *syntheticLayouts.get(&spawn.result_request).ok_or("fiber result layout missing")?;
        if fiberPayloads.insert(handle, payload).is_some_and(|previous| records[previous] != records[payload]) {
            return Err("fiber payload layout is ambiguous".into());
        }
    }
    for lowered in &mut artifact.functions {
        if dynamic.replace(lowered) { continue; }
        let function = &mut lowered.function;
        let mut descriptors = HashMap::new();
        for (index, array) in arrays.iter().enumerate() {
            for value in descriptors_named(function, &array.allocation_request_symbol) {
                descriptors.insert(value, Native::ArrayDescriptor(index));
            }
        }
        for (index, packet) in packets.iter().enumerate() {
            for value in descriptors_named(function, &packet.allocation_request_symbol) {
                let record = if let Some(record) = syntheticLayouts.get(&packet.allocation_request_symbol) { *record } else { artifact
                    .aggregate_declarations
                    .get(&packet.allocation_request_symbol)
                    .and_then(|declaration| nominalLayouts.get(declaration))
                    .copied()
                    .unwrap_or(index) };
                descriptors.insert(value, Native::PacketDescriptor(record));
            }
        }
        if layout.observers.is_some() {
            for (value, data) in function.global_values.iter() {
                if matches!(data, GlobalValueData::Symbol { name: ExternalName::TestCase(name), offset, tls: false, .. }
                    if name.raw() == OBSERVER_NODE_DESCRIPTOR_SYMBOL.as_bytes() && offset.bits() == 0)
                {
                    descriptors.insert(value, Native::ObserverDescriptor);
                }
            }
        }
        if let Some(record) = wrapperRecord {
            for (value, data) in function.global_values.iter() {
                if matches!(data, GlobalValueData::Symbol { name: ExternalName::TestCase(name), offset, tls: false, .. } if name.raw() == beskid_codegen::event_handler_static::EVENT_HANDLER_ALLOCATION_REQUEST_SYMBOL.as_bytes() && offset.bits() == 0) {
                    descriptors.insert(value, Native::PacketDescriptor(record));
                }
            }
        }
        for (request, descriptorSymbol, record) in &closureLayouts {
            for symbol in [*request, *descriptorSymbol] {
                for value in descriptors_named(function, symbol) {
                    descriptors.insert(value, Native::PacketDescriptor(*record));
                }
            }
        }
        for (value, data) in function.global_values.iter() {
            let GlobalValueData::Symbol { name: ExternalName::TestCase(name), offset, tls: false, .. } = data else {
                continue;
            };
            if offset.bits() == 0 {
                let symbol = std::str::from_utf8(name.raw())?;
                if symbol == GC_BARRIER_ACTIVE_SYMBOL {
                    descriptors.insert(value, Native::BarrierActive);
                }
                if let Some(record) = contractRecord {
                    use beskid_codegen::contract_static::{CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL, CONTRACT_BOX_SCALAR_REQUEST_SYMBOL};
                    if matches!(symbol, CONTRACT_BOX_REFERENCE_REQUEST_SYMBOL | CONTRACT_BOX_SCALAR_REQUEST_SYMBOL) { descriptors.insert(value, Native::PacketDescriptor(record)); }
                    if artifact.witness_tables.iter().any(|t| t.symbol == symbol) { descriptors.insert(value, Native::Witness); }
                }
                if let Some(index) = strings.literal_index(symbol) {
                    descriptors.insert(value, Native::StringLiteral(index));
                }
            }
        }
        if function.global_values.len() != descriptors.len() {
            let unknown = function.global_values.iter().filter(|(value, _)| !descriptors.contains_key(value)).map(|(_, data)| format!("{data:?}")).collect::<Vec<_>>();
            return Err(format!("unexpected native data in Wasm GC probe: {}", unknown.join(", ")).into());
        }
        if function.stack_limit.is_some() || !function.dynamic_stack_slots.is_empty() {
            return Err("unsupported stack layout in Wasm GC probe".into());
        }

        let transfer_slots = function.dfg.ext_funcs.values().filter_map(|external| {
            let ExternalName::TestCase(name) = &external.name else { return None; };
            crate::scheduler::find(std::str::from_utf8(name.raw()).ok()?)
        }).any(|service| service.transfer != crate::scheduler::Transfer::None);
        let mut externals = HashMap::new();
        let mut scalarHosts = HashSet::new();
        for (reference, external) in function.dfg.ext_funcs.iter() {
            let ExternalName::TestCase(raw) = &external.name else {
                return Err("unexpected external reference in Wasm GC lowering".into());
            };
            let name = std::str::from_utf8(raw.raw())?;
            let sig = &function.dfg.signatures[external.signature];
            let valid = if name == POLL || crate::is_clock_import(name) {
                sig.params.is_empty() && sig.returns == [AbiParam::new(I64)]
            } else if matches!(name, ARRAY_ALLOC | PACKET_ALLOC | CLOSURE_ALLOC | SYM_ARRAY_LEN) {
                sig.params == [AbiParam::new(I64)] && sig.returns == [AbiParam::new(I64)]
            } else if let Some((from, to)) = crate::foundation::float_bit_types(name) {
                artifact.corelib_service_symbols.contains(name)
                    && sig.params == [AbiParam::new(from)] && sig.returns == [AbiParam::new(to)]
            } else if let Some(operation) = crate::browser_heap::events::Operation::native(name) {
                !layout.events.is_empty() && artifact.corelib_service_symbols.contains(name) && operation.matches(sig)
            } else if layout.observers.is_some() && name == observers::SUBSCRIBE {
                sig.params == [AbiParam::new(I64); 5] && sig.returns == [AbiParam::new(I8)]
            } else if layout.observers.is_some() && name == observers::TAIL {
                sig.params == [AbiParam::new(I64)] && sig.returns == [AbiParam::new(I64)]
            } else if name == SAME_IDENTITY {
                artifact.corelib_service_symbols.contains(name)
                    && sig.params == [AbiParam::new(I64); 2] && sig.returns == [AbiParam::new(I64)]
            } else if let Some(service) = network::service(name) {
                service.admits(sig, &artifact.corelib_service_symbols)
            } else if name == crate::fibers::JOIN_VALUE {
                artifact.corelib_service_symbols.contains(name) && sig.params == [AbiParam::new(I64); 2] && sig.returns == [AbiParam::new(I8)]
            } else if crate::fibers::admit(name, sig, &artifact.corelib_service_symbols,
                if matches!(name, crate::fibers::STACK_CHECK | crate::fibers::STACK_OVERFLOW) {
                    artifact.spawn_artifacts.iter().any(|spawn| spawn.trampoline == lowered.name)
                } else { !artifact.spawn_artifacts.is_empty() })? {
                if !matches!(name, crate::fibers::STACK_CHECK | crate::fibers::STACK_OVERFLOW | crate::fibers::JOIN_MESSAGE | crate::fibers::SPAWN) { scalarHosts.insert(name.to_owned()); }
                true
            } else if name == crate::scheduler::ABI_CLEAR {
                (transfer_slots || !artifact.spawn_artifacts.is_empty())
                    && sig.params == [AbiParam::new(I64)] && sig.returns == [AbiParam::new(I8)]
            } else if name == crate::scheduler::ABI_INITIALIZE {
                transfer_slots && sig.params == [AbiParam::new(I64); 4] && sig.returns == [AbiParam::new(I8)]
            } else if name == CAPTURE_STORE {
                sig.params == [AbiParam::new(I64); 4] && sig.returns == [AbiParam::new(I8)]
            } else if matches!(name, ARRAY_GROW | ARRAY_TRY_GROW) {
                sig.params == [AbiParam::new(I64); 2] && sig.returns == [AbiParam::new(I64)]
            } else if name == ARRAY_WRITE_BARRIER {
                sig.params == [AbiParam::new(I64), AbiParam::new(I64)] && sig.returns == [AbiParam::new(I8)]
            } else if name == GC_WRITE_BARRIER_SYMBOL {
                sig.params == [AbiParam::new(I64); 2] && sig.returns.is_empty()
            } else if strings.accepts_import(name) {
                strings::accepts_signature(name, sig)
            } else if userNames.contains(name) {
                signatures.contains_key(name) && nativeSignatures.get(name) == Some(sig)
            } else if crate::stdio::admitted(name, sig, &artifact.corelib_service_symbols)?
                || crate::foundation::admitted(name, sig, &artifact.corelib_service_symbols)?
                || crate::protocol::admit(name, sig, &artifact.corelib_service_symbols)?
                || text_services::is_host_import(name, sig, &artifact.corelib_service_symbols)? {
                true
            } else if let Some(service) = crate::scheduler::find(name) {
                if service.transfer == crate::scheduler::Transfer::None { scalarHosts.insert(name.to_owned()); }
                service.admit(sig, &artifact.corelib_service_symbols, false)?
            } else if let Some(service) = crate::composition::service(name) {
                let proven = if name == crate::composition::EXIT { artifact.corelib_service_symbols.contains(name) }
                    else { artifact.browser_composition_hooks.is_some() };
                service.admit(sig, proven)?
            } else if suspension::is_host_import(name, sig, &artifact.corelib_service_symbols)?
                || math::is_host_import(name, sig, &artifact.extern_imports)?
            {
                scalarHosts.insert(name.to_owned());
                true
            } else {
                return Err(crate::diagnostics::unavailable(name, &lowered.name, &artifact.browser_coverage, &artifact.extern_imports).into());
            };
            if !valid {
                return Err(format!("unsupported native signature of {name}").into());
            }
            externals.insert(reference, (name.to_owned(), external.clone()));
        }
        let entry = function.layout.entry_block().ok_or("managed CLIF has no entry block")?;
        let cfg = ControlFlowGraph::with_function(function);
        let slotValues = spills::values_at_loads(function, &cfg);
        let writeBarriers = guarded_write_barriers(function, &cfg, &descriptors, &externals, &slotValues)?;
        let dominators = DominatorTree::with_function(function, &cfg);
        let instructions: Vec<_> = dominators
            .cfg_postorder()
            .iter()
            .rev()
            .copied()
            .chain(function.layout.blocks().filter(|&block| !dominators.is_reachable(block)))
            .flat_map(|block| function.layout.block_insts(block).map(move |inst| (block, inst)))
            .collect();

        function.dfg.ext_funcs.clear();
        let mut pass = Pass {
            function,
            dispatch: contracts::Dispatch { record: contractRecord, tables: &artifact.witness_tables, results: contract_results.get(lowered.name.as_str()), pointers: HashSet::new(), slots: HashMap::new(), receivers: HashMap::new() },
            descriptors,
            externals,
            writeBarriers,
            scalarHosts,
            imports: HashMap::new(),
            managed: HashMap::new(),
            incoming: HashMap::new(),
            slotValues,
            requests: array_requests::Requests::default(),
            allocations: &allocations,
            strings,
            fields: records.clone(),
            events: &layout.events,
            offsets: offsets.clone(),
            signatures: &signatures,
            returnKind: signatures[&lowered.name].1,
            joinResult: signatures[&lowered.name].0.iter().flatten().filter_map(|kind| match kind { ReferenceKind::Record(record) => fiberPayloads.get(record).copied(), _ => None }).next(),
            joinSlots: HashMap::new(),
            transferSlots: transfers::Slots::default(),
            transferType: transfer_receivers.get(&lowered.name).copied(),
            hasTransferSlots: transfer_slots,
            spawnEntry: artifact.spawn_artifacts.iter().any(|spawn| spawn.trampoline == lowered.name),
            observerNode: layout.observers.as_ref().map(|observers| observers.node),
            wrapperRecord, captureStores: &captureStores, spawnEntries: &spawnEntries, spawnPointers: HashSet::new(), closureCalls: closure_calls.get(lowered.name.as_str()), callableSites: artifact.callable_sites.get(&lowered.name), callPointers: HashSet::new(), codePointers: HashSet::new(),
        };
        let parameters = pass.function.dfg.block_params(entry).to_vec();
        for (value, record) in parameters.into_iter().zip(&signatures[&lowered.name].0) {
            if let Some(record) = record {
                let mut cursor = FuncCursor::new(pass.function);
                cursor.goto_first_insertion_point(entry);
                let handle = cursor.ins().ireduce(I32, value);
                pass.managed.insert(value, record.bind(handle));
            }
        }
        let mut currentBlock = entry;
        for (block, instruction) in &instructions {
            if *block != currentBlock {
                pass.enterBlock(*block);
                currentBlock = *block;
            }
            match pass.rewrite(*instruction) {
                Ok(Edit::Keep) => {}
                Ok(Edit::Replace) => {}
                Ok(Edit::Erase) => pass.function.layout.remove_inst(*instruction),
                Err(reason) => {
                    let dfg = &pass.function.dfg;
                    let operands = dfg.inst_args(*instruction).iter().map(|&value| match dfg.value_def(value) {
                        ValueDef::Result(definition, _) => dfg.display_inst(definition).to_string(),
                        _ => value.to_string(),
                    }).collect::<Vec<_>>().join(", ");
                    return Err(format!(
                        "Wasm GC lowering in {}, {block}, {}: {reason} (operands: {operands})", lowered.name,
                        dfg.display_inst(*instruction)
                    )
                    .into());
                }
            }
        }
        contracts::retain_witness_globals(pass.function);
        pass.function.sized_stack_slots.clear();
        verify_function(pass.function, &settings::Flags::new(settings::builder()))?;
    }
    dynamic.finish(&mut artifact.functions);
    Ok(layout)
}

fn scalar(ty: SemanticTypeId) -> Option<Scalar> {
    match ty {
        SemanticTypeId::BOOL | SemanticTypeId::I8 | SemanticTypeId::U8 => Some(Scalar::I8),
        SemanticTypeId::I16 | SemanticTypeId::U16 => Some(Scalar::I16),
        SemanticTypeId::I32 | SemanticTypeId::U32 | SemanticTypeId::CHAR => Some(Scalar::I32),
        SemanticTypeId::I64 | SemanticTypeId::U64 | SemanticTypeId::WORD => Some(Scalar::I64),
        SemanticTypeId::F32 => Some(Scalar::F32),
        SemanticTypeId::F64 => Some(Scalar::F64),
        _ => None,
    }
}

fn raw_source_type(ty: &SourceSignatureType) -> Result<cranelift_codegen::ir::Type, Box<dyn Error>> {
    match ty {
        SourceSignatureType::Scalar(SemanticTypeId::STRING) => Ok(I64),
        SourceSignatureType::Scalar(ty) => scalar(*ty).map(scalar_type).ok_or_else(|| "unsupported scalar signature".into()),
        SourceSignatureType::Nominal(_) | SourceSignatureType::Array(_) | SourceSignatureType::Function { .. } => Ok(I64),
    }
}

fn array_element(ty: SemanticTypeId) -> Option<ArrayElement> {
    if ty == SemanticTypeId::STRING { Some(ArrayElement::String) } else { scalar(ty).map(ArrayElement::Scalar) }
}

fn native_element_type(element: ArrayElement) -> cranelift_codegen::ir::Type {
    match element {
        ArrayElement::Scalar(scalar) => scalar_type(scalar),
        ArrayElement::String | ArrayElement::Record(_) | ArrayElement::Nested { .. } => I64,
    }
}

fn array_managed_value(element: ArrayElement, value: Option<Native>) -> Option<Value> {
    match (element, value?) {
        (ArrayElement::String, Native::String(handle)) => Some(handle),
        (ArrayElement::Record(expected), Native::Packet(handle, actual)) if expected == actual => Some(handle),
        (element, Native::Array(handle, actual)) if element.inner() == Some(actual) => Some(handle),
        _ => None,
    }
}

fn descriptors_named<'a>(function: &'a Function, symbol: &'a str) -> impl Iterator<Item = GlobalValue> + 'a {
    function.global_values.iter().filter_map(move |(value, data)| match data {
        GlobalValueData::Symbol { name: ExternalName::TestCase(name), offset, tls: false, .. }
            if name.raw() == symbol.as_bytes() && offset.bits() == 0 =>
        {
            Some(value)
        }
        _ => None,
    })
}

#[derive(Default)]
struct GuardedWriteBarriers {
    calls: HashSet<Inst>,
    probes: HashSet<Inst>,
}

fn guarded_write_barriers(
    function: &Function,
    cfg: &ControlFlowGraph,
    descriptors: &HashMap<GlobalValue, Native>,
    externals: &HashMap<FuncRef, (String, ExtFuncData)>,
    spills: &HashMap<Inst, Value>,
) -> Result<GuardedWriteBarriers, &'static str> {
    use InstructionData::*;
    let dfg = &function.dfg;
    let definition = |value| match dfg.value_def(dfg.resolve_aliases(value)) {
        ValueDef::Result(inst, _) => Some(inst),
        _ => None,
    };
    let targets = |inst| dfg.insts[inst]
        .branch_destination(&dfg.jump_tables, &dfg.exception_tables)
        .iter()
        .map(|target| target.block(&dfg.value_lists));
    let substantive = |inst| dfg.insts[inst].opcode() != Opcode::StackAddr && !spills.contains_key(&inst);
    let unspill = |mut value| {
        for _ in 0..=spills.len() {
            value = dfg.resolve_aliases(value);
            match definition(value).and_then(|inst| spills.get(&inst)) {
                Some(&stored) => value = stored,
                None => return Some(value),
            }
        }
        None
    };
    let mut result = GuardedWriteBarriers::default();
    for block in function.layout.blocks() {
        for inst in function.layout.block_insts(block) {
            let Call { func_ref, .. } = dfg.insts[inst] else { continue };
            if externals[&func_ref].0 != GC_WRITE_BARRIER_SYMBOL { continue; }
            let matched = (|| {
                let [slot, value] = dfg.inst_args(inst) else { return None };
                let mut body = function.layout.block_insts(block).filter(|&inst| substantive(inst));
                if body.next()? != inst { return None; }
                let jump = body.next()?;
                if dfg.insts[jump].opcode() != Opcode::Jump || body.next().is_some() { return None; }
                let successor = targets(jump).next()?;
                let store = function.layout.block_insts(successor).find(|&inst| substantive(inst))?;
                if !matches!(dfg.insts[store], Store { opcode: Opcode::Store, args, offset, .. }
                    if unspill(args[0])? == unspill(*value)? && unspill(args[1])? == unspill(*slot)?
                        && i32::from(offset) == 0) { return None; }

                let mut predecessors = cfg.pred_iter(block);
                let branch = predecessors.next()?.inst;
                if predecessors.next().is_some() || dfg.insts[branch].opcode() != Opcode::Brif
                    || !targets(branch).eq([block, successor]) { return None; }
                let [condition] = dfg.inst_args(branch) else { return None };
                let probe = definition(*condition)?;
                let Load { opcode: Opcode::Load, arg, offset, .. } = dfg.insts[probe] else { return None };
                if i32::from(offset) != 0 { return None; }
                let UnaryGlobalValue { opcode: Opcode::SymbolValue, global_value } = dfg.insts[definition(arg)?]
                    else { return None };
                (descriptors.get(&global_value) == Some(&Native::BarrierActive)).then_some(probe)
            })();
            let probe = matched.ok_or("unrecognized native managed write barrier control flow")?;
            result.calls.insert(inst);
            result.probes.insert(probe);
        }
    }
    Ok(result)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Native {
    Witness,
    BarrierActive,
    ArrayDescriptor(usize),
    ObserverDescriptor,
    StaticRequestWord { plan: usize, word: usize },
    RequestSlot(StackSlot, usize),
    PacketDescriptor(usize),
    StringLiteral(usize),
    Slot,
    FiberSlot(StackSlot, i32),
    TransferDescriptor(usize),
    Received(Value),
    Packet(Value, usize),
    String(Value),
    Environment(Value),
    StringData(Value),
    StringByte(Value, Value),
    Array(Value, ArrayElement),
    ArrayData(Value, ArrayElement),
    ArrayLength(Value, ArrayElement),
    ArraySharing(Value, ArrayElement),
    PacketField(Value, usize, i64),
    Element(Value, Index, ArrayElement),
    Lifetime,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceKind {
    String,
    Environment,
    Record(usize),
    Array(ArrayElement),
}

struct CallableShape {
    parameters: Vec<(cranelift_codegen::ir::Type, Option<ReferenceKind>)>,
    result: Option<(cranelift_codegen::ir::Type, Option<ReferenceKind>)>,
}

impl CallableShape {
    fn matches(&self, signature: &Signature, environment: bool) -> bool {
        signature.params.iter().map(|param| param.value_type)
            .eq(environment.then_some(I64).into_iter().chain(self.parameters.iter().map(|&(ty, _)| ty)))
            && signature.returns.iter().map(|param| param.value_type).eq(self.result.map(|(ty, _)| ty))
    }
}

impl ReferenceKind {
    fn of(source: Native) -> Option<(Self, Value)> {
        match source {
            Native::String(handle) => Some((Self::String, handle)),
            Native::Environment(handle) => Some((Self::Environment, handle)),
            Native::Packet(handle, record) => Some((Self::Record(record), handle)),
            Native::Array(handle, element) => Some((Self::Array(element), handle)),
            _ => None,
        }
    }

    fn bind(self, handle: Value) -> Native {
        match self {
            Self::String => Native::String(handle),
            Self::Environment => Native::Environment(handle),
            Self::Record(layout) => Native::Packet(handle, layout),
            Self::Array(element) => Native::Array(handle, element),
        }
    }

    fn handle(self, source: Native) -> Option<Value> {
        match (self, source) {
            (Self::String, Native::String(handle)) | (Self::Environment, Native::Environment(handle)) => Some(handle),
            (Self::Record(layout), Native::Packet(handle, actual)) if layout == actual => Some(handle),
            (Self::Array(element), Native::Array(handle, actual)) if element == actual => Some(handle),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BranchKind {
    Zero,
    Scalar,
    Reference(ReferenceKind),
}

impl BranchKind {
    fn merge(self, next: Self) -> Option<Self> {
        match (self, next) {
            (Self::Zero, kind) | (kind, Self::Zero) => Some(kind),
            (left, right) if left == right => Some(left),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Index {
    Constant(i32),
    Scalar(Value),
    Word(Value),
}

enum Edit {
    Keep,
    Erase,
    Replace,
}

struct Pass<'a> {
    dispatch: contracts::Dispatch<'a>,
    function: &'a mut Function,
    descriptors: HashMap<GlobalValue, Native>,
    externals: HashMap<FuncRef, (String, ExtFuncData)>,
    writeBarriers: GuardedWriteBarriers,
    scalarHosts: HashSet<String>,
    imports: HashMap<String, FuncRef>,
    managed: HashMap<Value, Native>,
    incoming: HashMap<Value, BranchKind>,
    slotValues: HashMap<Inst, Value>,
    requests: array_requests::Requests,
    allocations: &'a [(i32, ArrayElement)],
    strings: &'a StringPool,
    fields: Vec<Vec<Field>>,
    events: &'a [crate::browser_heap::events::Event],
    offsets: Vec<Vec<i64>>,
    signatures: &'a HashMap<String, (Vec<Option<ReferenceKind>>, Option<ReferenceKind>)>,
    returnKind: Option<ReferenceKind>,
    spawnEntry: bool,
    joinResult: Option<usize>,
    joinSlots: HashMap<(StackSlot, Block), Value>,
    transferSlots: transfers::Slots,
    transferType: Option<(cranelift_codegen::ir::Type, Option<ReferenceKind>)>,
    hasTransferSlots: bool,
    wrapperRecord: Option<usize>,
    observerNode: Option<usize>,
    captureStores: &'a HashMap<usize, Vec<usize>>,
    spawnEntries: &'a HashMap<String, i64>,
    spawnPointers: HashSet<Value>,
    closureCalls: Option<&'a HashMap<Inst, CallableShape>>,
    callableSites: Option<&'a HashMap<Inst, CallableSite>>,
    callPointers: HashSet<Value>,
    codePointers: HashSet<Value>,
}

impl Pass<'_> {
    fn rewrite(&mut self, inst: Inst) -> Result<Edit, &'static str> {
        use Edit::{Keep, Replace};
        use InstructionData::*;
        use Native::*;
        let dfg = &self.function.dfg;
        let data = dfg.insts[inst].clone();
        match data {
            UnaryGlobalValue { opcode: Opcode::SymbolValue, global_value } => {
                let descriptor = self.descriptors.get(&global_value).copied().ok_or("unknown allocation descriptor")?;
                if matches!(descriptor, Native::Witness) {
                    let pointer = self.rewindBefore(inst).ins().symbol_value(I32, global_value);
                    self.function.replace(inst).uextend(I64, pointer);
                    self.dispatch.pointers.insert(self.function.dfg.first_result(inst));
                    return Ok(Replace);
                }
                self.eraseResult(inst, descriptor)
            }
            FuncAddr { func_ref, .. } => {
                let (name, external) = self.externals.get(&func_ref).ok_or("unknown function address")?.clone();
                if let Some(id) = self.spawnEntries.get(&name).copied() {
                    let result = self.function.dfg.first_result(inst);
                    self.spawnPointers.insert(result);
                    let ty = self.function.dfg.value_type(result);
                    self.function.replace(inst).iconst(ty, id);
                    return Ok(Replace);
                }
                if !matches!(self.callableSites.and_then(|sites| sites.get(&inst)), Some(CallableSite::ClosureCodeAddress(_)))
                    || !self.signatures.contains_key(&name) {
                    return Err("function address is not a proven closure entry");
                }
                let new_ref = self.function.import_function(external.clone());
                self.codePointers.insert(self.function.dfg.first_result(inst));
                let ty = self.function.dfg.value_type(self.function.dfg.first_result(inst));
                self.function.replace(inst).func_addr(ty, new_ref);
                Ok(Replace)
            }
            Call { func_ref, .. } => self.rewriteCall(inst, func_ref),
            CallIndirect { opcode: Opcode::CallIndirect, sig_ref, .. } => self.rewriteIndirect(inst, sig_ref),
            IntCompare { opcode: Opcode::Icmp, cond, args: [left, right] } if matches!(self.source(left), Some(Environment(_))) => {
                let Some(Environment(handle)) = self.source(left) else { unreachable!() };
                if self.constant(right) != Some(0) { return Err("unsupported closure environment comparison"); }
                let wide = self.rewindBefore(inst).ins().uextend(I64, handle);
                self.function.replace(inst).icmp(cond, wide, right);
                Ok(Replace)
            }
            IntCompare { opcode: Opcode::Icmp, cond, args: [left, right] }
                if self.source(left).and_then(ReferenceKind::of).is_some()
                    || self.source(right).and_then(ReferenceKind::of).is_some() =>
            {
                use cranelift_codegen::ir::condcodes::IntCC::{Equal, NotEqual};
                if !matches!(cond, Equal | NotEqual) {
                    return Err("unsupported ordering comparison of managed references");
                }
                let operand = |pass: &mut Self, value| {
                    if let Some((_, handle)) = pass.source(value).and_then(ReferenceKind::of) {
                        return Some(handle);
                    }
                    (pass.constant(value) == Some(0)).then(|| pass.rewindBefore(inst).ins().iconst(I32, 0))
                };
                let left = operand(self, left).ok_or("unproven managed equality operand")?;
                let right = operand(self, right).ok_or("unproven managed equality operand")?;
                let ty = self.function.dfg.value_type(self.function.dfg.first_result(inst));
                let call = self.callBefore(inst, crate::browser_heap::REFERENCE_EQUAL, &[left, right], Some(ty));
                if cond == NotEqual {
                    let value = self.function.dfg.first_result(call);
                    self.function.replace(inst).bxor_imm_u(value, 1);
                    Ok(Replace)
                } else {
                    self.function.dfg.replace_with_aliases(inst, call);
                    Ok(Edit::Erase)
                }
            }
            _ if matches!(data.opcode(), Opcode::Jump | Opcode::Brif) => self.rewriteBranch(inst),
            _ if data.opcode() == Opcode::Return && self.returnKind.is_some() => {
                let [value] = self.function.dfg.inst_args(inst) else {
                    return Err("invalid managed return");
                };
                if self.spawnEntry && self.constant(*value) == Some(0) { return Ok(Keep); }
                let handle = self
                    .returnKind
                    .and_then(|kind| kind.handle(self.source(*value)?))
                    .ok_or("unproven managed return type")?;
                let value = self.rewindBefore(inst).ins().uextend(I64, handle);
                self.function.replace(inst).return_(&[value]);
                Ok(Replace)
            }
            StackAddr { stack_slot, offset, .. } => {
                let slot = &self.function.sized_stack_slots[stack_slot];
                let offset = i32::from(offset);
                let source = match (slot.size, slot.align_shift, offset) {
                    (8, 3, 0) => Slot,
                    (40, 3, offset) if (self.joinResult.is_some() || self.hasTransferSlots) && (0..40).contains(&offset) && offset % 8 == 0 => FiberSlot(stack_slot, offset),
                    (32, 3, offset) => RequestSlot(
                        stack_slot,
                        array_requests::word_index(offset).ok_or("unexpected native array request word")?,
                    ),
                    _ => return Err("unsupported native stack slot layout"),
                };
                self.eraseResult(inst, source)
            }
            Load { opcode: Opcode::Load, arg, offset, .. } => self.load(inst, arg, i32::from(offset)),
            Store { opcode: Opcode::Store, args: [value, address], offset, .. } => {
                let offset = i64::from(i32::from(offset));
                let destination = match self.source(address) {
                    Some(Array(handle, element)) if offset == 8 => Some(ArrayLength(handle, element)),
                    Some(Packet(handle, layout)) => Some(PacketField(handle, layout, offset)),
                    other if offset == 0 => other,
                    _ => return Err("unsupported native store offset"),
                };
                self.store(inst, value, destination)
            }
            Ternary { opcode: Opcode::Select, args: [condition, left, right] }
                if [left, right].iter().any(|&value| self.source(value).and_then(ReferenceKind::of).is_some()) =>
            {
                let kind = [left, right].into_iter().find_map(|value| self.source(value).and_then(ReferenceKind::of))
                    .ok_or("managed select lacks a source type")?.0;
                let mut operand = |value| match self.source(value) {
                    Some(source) => kind.handle(source).ok_or("managed select operands differ in type"),
                    None if self.constant(value) == Some(0) => Ok(self.rewindBefore(inst).ins().iconst(I32, 0)),
                    None => Err("managed select operand is neither a reference nor null"),
                };
                let left = operand(left)?;
                let right = operand(right)?;
                let condition = match self.source(condition) {
                    None => condition,
                    Some(source) => ReferenceKind::of(source).ok_or("native address used as a select condition")?.1,
                };
                let handle = self.rewindBefore(inst).ins().select(condition, left, right);
                self.eraseResult(inst, kind.bind(handle))
            }
            Ternary { opcode: Opcode::Select, args: [_, reference, scalar] }
                if matches!(self.source(reference), Some(PacketDescriptor(id)) if Some(id) == self.dispatch.record && self.source(reference) == self.source(scalar)) =>
            {
                self.eraseResult(inst, self.source(reference).unwrap())
            }
            Ternary { opcode: Opcode::Select, args: [test, address, scratch] } if matches!(self.source(address), Some(ArraySharing(_, _))) => {
                match (self.source(test), self.source(address), self.source(scratch)) {
                    (Some(Array(a, element)), Some(ArraySharing(b, kind)), Some(Slot)) if a == b && element == kind => self.eraseResult(inst, ArraySharing(a, element)),
                    _ => Err("invalid guarded array sharing address"),
                }
            }
            Binary { opcode: Opcode::Iadd, args: [base, offset] } => match self.source(base) {
                Some(Array(handle, element)) if self.constant(offset) == Some(-8) => self.eraseResult(inst, ArraySharing(handle, element)),
                Some(StringData(handle))
                    if self.function.dfg.value_type(offset) == I64 && self.source(offset).is_none() =>
                {
                    self.eraseResult(inst, StringByte(handle, offset))
                }
                Some(Packet(handle, record)) => {
                    let offset = self.constant(offset).ok_or("dynamic record field offset")?;
                    self.eraseResult(inst, PacketField(handle, record, offset))
                }
                Some(ArrayData(handle, element)) => {
                    let index = self.index(offset, element).ok_or("unknown array element address calculation")?;
                    self.eraseResult(inst, Element(handle, index, element))
                }
                None if self.source(offset).is_none() => Ok(Keep),
                _ => Err("unexpected managed address arithmetic"),
            },
            Binary { opcode: Opcode::Bor, args: [left, right] } if self.source(left).is_some() => {
                match (self.source(left), self.source(right)) {
                    (Some(Array(a, left)), Some(Array(b, right))) if a == b && left == right => {
                        self.eraseResult(inst, Lifetime)
                    }
                    (Some(String(a)), Some(String(b))) if a == b => self.eraseResult(inst, Lifetime),
                    (Some(Packet(a, ar)), Some(Packet(b, br))) if a == b && ar == br => {
                        self.eraseResult(inst, Lifetime)
                    }
                    _ => Err("unexpected managed lifetime expression"),
                }
            }
            CondTrap { opcode: Opcode::Trapz, arg, code } => match self.source(arg) {
                Some(Array(handle, _)) | Some(Packet(handle, _)) | Some(String(handle)) => {
                    self.function.replace(inst).trapz(handle, code);
                    Ok(Replace)
                }
                None => Ok(Keep),
                _ => Err("unexpected managed null check"),
            },
            _ if dfg.inst_values(inst).any(|value| self.source(value).is_some()) => {
                Err("managed reference escaped the supported Wasm GC operations")
            }
            _ if dfg.user_stack_map_entries(inst).is_some()
                || data.opcode().can_load()
                || data.opcode().can_store() =>
            {
                Err("unsupported native memory operation")
            }
            _ if matches!(data.opcode(), Opcode::StackAddr | Opcode::DynamicStackAddr) => {
                Err("unsupported native stack slot address")
            }
            UnaryGlobalValue { .. } => Err("unsupported native global value access"),
            _ if data.opcode().is_call() => Err("unsupported indirect native call"),
            _ => Ok(Keep),
        }
    }

    fn enterBlock(&mut self, block: Block) {
        let params: Vec<_> = self
            .function
            .dfg
            .block_params(block)
            .iter()
            .filter_map(|&value| match self.incoming.get(&value) {
                Some(BranchKind::Reference(kind)) => Some((value, *kind)),
                _ => None,
            })
            .collect();
        let mut cursor = FuncCursor::new(self.function);
        cursor.goto_first_insertion_point(block);
        for (value, kind) in params {
            let handle = cursor.ins().ireduce(I32, value);
            self.managed.insert(value, kind.bind(handle));
        }
    }

    fn rewriteBranch(&mut self, inst: Inst) -> Result<Edit, &'static str> {
        if let &[condition] = self.function.dfg.inst_args(inst)
            && let Some(source) = self.source(condition)
        {
            let (_, handle) = ReferenceKind::of(source).ok_or("native address used as a branch condition")?;
            self.function.dfg.inst_args_mut(inst)[0] = handle;
        }
        let dfg = &self.function.dfg;
        let mut edges = Vec::new();
        for branch in dfg.insts[inst].branch_destination(&dfg.jump_tables, &dfg.exception_tables) {
            let params = dfg.block_params(branch.block(&dfg.value_lists));
            for (&param, arg) in params.iter().zip(branch.args(&dfg.value_lists)) {
                let value = arg.as_value().ok_or("unsupported branch argument")?;
                let reference = match self.source(value) {
                    Some(source) => Some(ReferenceKind::of(source).ok_or("native address escaped through a branch")?),
                    None => None,
                };
                edges.push((param, value, reference));
            }
        }
        let mut replacements = HashMap::new();
        for (param, value, reference) in edges {
            let kind = match reference {
                Some((kind, _)) => BranchKind::Reference(kind),
                None if self.function.dfg.value_type(value) == I64 && self.constant(value) == Some(0) => BranchKind::Zero,
                None => BranchKind::Scalar,
            };
            let previous = self.incoming.entry(param).or_insert(kind);
            *previous = previous.merge(kind).ok_or("incompatible managed branch argument types")?;
            if let Some((_, handle)) = reference {
                if !replacements.contains_key(&value) {
                    let packed = self.rewindBefore(inst).ins().uextend(I64, handle);
                    replacements.insert(value, packed);
                }
            }
        }
        self.function.dfg.map_inst_values(inst, |value| replacements.get(&value).copied().unwrap_or(value));
        Ok(Edit::Keep)
    }

    fn rewindBefore(&mut self, inst: Inst) -> FuncCursor<'_> {
        let mut cursor = FuncCursor::new(self.function);
        cursor.goto_inst(inst);
        cursor
    }

    fn rewriteCall(&mut self, inst: Inst, reference: FuncRef) -> Result<Edit, &'static str> {
        use Native::*;
        let (name, external) = self.externals.get(&reference).ok_or("unknown native call")?.clone();
        let args = self.function.dfg.inst_args(inst);
        if let Some((_, target)) = crate::foundation::float_bit_types(&name) {
            let &[value] = args else { return Err("float bit projection requires one operand"); };
            self.function.replace(inst).bitcast(target, cranelift_codegen::ir::MemFlagsData::new(), value);
            return Ok(Edit::Replace);
        }
        if let Some(operation) = crate::browser_heap::events::Operation::native(&name) {
            return self.event_call(inst, operation, &args.to_vec());
        }
        if let Some(service) = network::service(&name) {
            use network::Argument::*;
            let mut translated = Vec::with_capacity(args.len());
            for (argument, kind) in args.to_vec().into_iter().zip(service.arguments) {
                match (kind, self.source(argument)) {
                    (String, Some(Native::String(handle)))
                    | (InputBytes | OutputBytes, Some(Array(handle, ArrayElement::Scalar(Scalar::I8))))
                    | (ResultHandle, Some(Array(handle, ArrayElement::Scalar(Scalar::I64)))) => {
                        translated.push(self.rewindBefore(inst).ins().uextend(I64, handle));
                    }
                    (Word | Integer | Flag, None) => translated.push(argument),
                    _ => return Err("invalid managed argument to browser networking service"),
                }
            }
            return self.stringCall(inst, external, &translated, false);
        }
        if name == crate::scheduler::ABI_CLEAR || self.hasTransferSlots {
            let transfer_args = args.to_vec();
            if let Some(result) = self.transfer_call(inst, &name, &transfer_args) {
                return result;
            }
        }
        let args = self.function.dfg.inst_args(inst);
        match (name.as_str(), args) {
            (observers::SUBSCRIBE, [owner, slot, property, callback, descriptor]) => {
                let (owner, slot, property, callback, descriptor) = (*owner, *slot, *property, *callback, *descriptor);
                if self.source(descriptor) != Some(ObserverDescriptor) {
                    return Err("observation registration requires its compiler descriptor");
                }
                let Some(Packet(callback, wrapper)) = self.source(callback) else {
                    return Err("observation registration requires a closure record");
                };
                if Some(wrapper) != self.wrapperRecord {
                    return Err("invalid observation callback layout");
                }
                let owner = if let Some(Packet(owner, _)) = self.source(owner) {
                    owner
                } else if let Some((&record, &receiver)) = self.dispatch.record.as_ref().zip(self.dispatch.receivers.get(&self.function.dfg.resolve_aliases(owner))) {
                    let call = self.callBefore(inst, &RecordOp::Get(0).symbol(record), &[receiver], Some(I32));
                    self.function.dfg.first_result(call)
                } else {
                    return Err("observation registration requires a managed owner");
                };
                self.replaceCall(inst, observers::WASM_SUBSCRIBE, &[owner, slot, property, callback], I8)
            }
            (observers::TAIL, [head]) => {
                let Some(Packet(head, node)) = self.source(*head) else {
                    return Err("observer tail requires an observer node");
                };
                if Some(node) != self.observerNode {
                    return Err("invalid observer node layout");
                }
                let call = self.callBefore(inst, observers::WASM_TAIL, &[head], Some(I32));
                self.eraseResult(inst, Packet(self.function.dfg.first_result(call), node))
            }
            (SAME_IDENTITY, [left, right]) => {
                let (kind, left) = self.source(*left).and_then(ReferenceKind::of).ok_or("managed identity requires a reference")?;
                let right = self.source(*right).and_then(|source| kind.handle(source)).ok_or("managed identity operands differ in type")?;
                self.replaceCall(inst, crate::browser_heap::IDENTITY_STATUS, &[left, right], I64)
            }
            (crate::fibers::JOIN_VALUE, [handle, slot]) => {
                let handle = *handle;
                let Some(FiberSlot(slot, 0)) = self.source(*slot) else { return Err("fiber result destination is not a proven ABI slot"); };
                let record = self.joinResult.ok_or("fiber result source type is unavailable")?;
                let block = self.function.layout.inst_block(inst).ok_or("fiber join value has no block")?;
                let mut signature = Signature::new(self.function.signature.call_conv);
                signature.params.push(AbiParam::new(I64));
                signature.returns.push(AbiParam::new(I64));
                let signature = self.function.import_signature(signature);
                let import = self.function.import_function(ExtFuncData { name: ExternalName::testcase(crate::fibers::JOIN_VALUE), signature, colocated: false, patchable: false });
                let call = self.rewindBefore(inst).ins().call(import, &[handle]);
                let result = self.function.dfg.first_result(call);
                let result = self.rewindBefore(inst).ins().ireduce(I32, result);
                let _ = record;
                if self.joinSlots.insert((slot, block), result).is_some() { return Err("fiber ABI slot reused without clearing"); }
                self.function.replace(inst).iconst(I8, 1);
                Ok(Edit::Replace)
            }
            (crate::fibers::STACK_CHECK, [_]) => { self.function.replace(inst).iconst(I8, 1); Ok(Edit::Replace) }
            (crate::fibers::STACK_OVERFLOW, []) => Ok(Edit::Erase),
            (crate::fibers::JOIN_MESSAGE, [handle]) => { let handle = *handle; self.stringCall(inst, external, &[handle], true) }
            ("fiber_spawn", [entry, environment]) => {
                let entry = *entry;
                let environment = *environment;
                if !self.spawnPointers.contains(&self.function.dfg.resolve_aliases(entry)) { return Err("spawn target is not a proven trampoline"); }
                let environment = match self.source(environment) {
                    Some(Packet(handle, _)) => self.rewindBefore(inst).ins().uextend(I64, handle),
                    None if self.constant(environment) == Some(0) => environment,
                    _ => return Err("spawn environment is not a proven managed reference"),
                };
                self.stringCall(inst, external, &[entry, environment], false)
            }
            (CAPTURE_STORE, [environment, descriptor, index, value]) => {
                let Some(Packet(handle, record)) = self.source(*environment) else { return Err("capture store requires an environment"); };
                if !matches!(self.source(*descriptor), Some(PacketDescriptor(id)) if id == record) { return Err("capture descriptor differs from environment"); }
                let map_index = self.constant(*index).and_then(|v| usize::try_from(v).ok()).ok_or("capture map index is not constant")?;
                let field = *self.captureStores.get(&record).and_then(|fields| fields.get(map_index)).ok_or("capture map index out of bounds")?;
                let value = match (self.fields[record][field], self.source(*value)) {
                    (Field::Record(expected), Some(Packet(value, actual))) if expected == actual => value,
                    (Field::Array(expected), Some(Array(value, actual))) if expected == actual => value,
                    (Field::String, Some(String(value))) => value,
                    _ => return Err("capture store source type mismatch"),
                };
                self.callBefore(inst, &RecordOp::Set(field).symbol(record), &[handle, value], None);
                self.function.replace(inst).iconst(I8, 1);
                Ok(Edit::Replace)
            }
            (ARRAY_GROW | ARRAY_TRY_GROW, [owner, minimum]) => {
                let Some(Array(handle, element)) = self.source(*owner) else { return Err("array grow requires a managed array"); };
                let (operation, size) = if name == ARRAY_TRY_GROW {
                    (ArrayOp::TryGrow, *minimum)
                } else {
                    (ArrayOp::Grow, self.materializeIndex(inst, Index::Word(*minimum)))
                };
                let call = self.callBefore(inst, &operation.symbol(element), &[handle, size], Some(I32));
                self.eraseResult(inst, Array(self.function.dfg.first_result(call), element))
            }
            (ARRAY_ALLOC, [request]) => {
                let (plan, runtimeLength) = match self.source(*request) {
                    Some(ArrayDescriptor(plan)) => (plan, None),
                    Some(RequestSlot(slot, 0)) => {
                        let block = self.function.layout.inst_block(inst).ok_or("array call without a block")?;
                        let (plan, length) = self.requests.take(block, slot)?;
                        (plan, Some(length))
                    }
                    _ => return Err("invalid native array request"),
                };
                let (staticLength, element) = self.allocations[plan];
                let length = if let Some(length) = runtimeLength {
                    if staticLength != 0 {
                        return Err("sized array request must start with a zero length");
                    }
                    self.materializeIndex(inst, Index::Word(length))
                } else {
                    self.rewindBefore(inst).ins().iconst(I32, i64::from(staticLength))
                };
                let array = self.callBefore(inst, &ArrayOp::Alloc.symbol(element), &[length], Some(I32));
                let array = self.function.dfg.first_result(array);
                self.eraseResult(inst, Array(array, element))
            }
            (SYM_ARRAY_LEN, [owner]) => {
                let Array(handle, element) = self.source(*owner).ok_or("length requires a managed array")? else {
                    return Err("length requires a managed array");
                };
                self.arrayLength(inst, handle, element)
            }
            (PACKET_ALLOC | CLOSURE_ALLOC, [request]) if matches!(self.source(*request), Some(PacketDescriptor(_))) => {
                let Some(PacketDescriptor(record)) = self.source(*request) else { unreachable!() };
                let packet = self.callBefore(inst, &RecordOp::Alloc.symbol(record), &[], Some(I32));
                let packet = self.function.dfg.first_result(packet);
                self.eraseResult(inst, Packet(packet, record))
            }
            (ARRAY_WRITE_BARRIER, [array, value]) => {
                let Some(Array(_, element)) = self.source(*array) else {
                    return Err("unproven write barrier for a managed array");
                };
                let word_scalar = element == ArrayElement::Scalar(Scalar::I64)
                    && self.source(*value).is_none() && self.function.dfg.value_type(*value) == I64;
                if !word_scalar && array_managed_value(element, self.source(*value)).is_none() {
                    return Err("write barrier value does not match the managed array element type");
                }
                // The engine applies its write barrier in array.set.
                let success = self.rewindBefore(inst).ins().iconst(I8, 1);
                let ValueDef::Result(replacement, _) = self.function.dfg.value_def(success) else {
                    return Err("missing array write barrier result");
                };
                self.function.dfg.replace_with_aliases(inst, replacement);
                Ok(Edit::Erase)
            }
            (GC_WRITE_BARRIER_SYMBOL, [slot, value]) if self.writeBarriers.calls.contains(&inst) => {
                let isReferenceSlot = match self.source(*slot) {
                    Some(Element(_, _, element)) => !matches!(element, ArrayElement::Scalar(_)),
                    Some(PacketField(_, record, offset)) => {
                        self.offsets[record].iter().position(|field| *field == offset)
                            .is_some_and(|index| !matches!(self.fields[record][index], Field::Scalar(_)))
                    }
                    _ => false,
                };
                if !isReferenceSlot || (self.source(*value).and_then(ReferenceKind::of).is_none() && self.constant(*value) != Some(0)) {
                    return Err("unproven managed write barrier operands");
                }
                // The identical store in the successor becomes struct.set or array.set.
                Ok(Edit::Erase)
            }
            (SYM_STR_NEW, [literal, length]) => {
                let StringLiteral(index) =
                    self.source(*literal).ok_or("string construction requires a proven literal")?
                else {
                    return Err("string construction requires a proven literal");
                };
                let length = *length;
                let lengthValue = self.constant(length).ok_or("nonconstant string literal size")?;
                if !self.strings.literal_matches_length(index, lengthValue) {
                    return Err("string literal size differs from canonical byte length");
                }
                let literal = self.rewindBefore(inst).ins().iconst(I64, index as i64);
                self.stringCall(inst, external, &[literal, length], true)
            }
            (host, [fd, data, length]) if crate::stdio::handles(host) => {
                let Some(ArrayData(buffer, ArrayElement::Scalar(Scalar::I8))) = self.source(*data) else {
                    return Err("byte stream service requires a proven byte array");
                };
                if self.source(*fd).is_some() || self.source(*length).is_some() {
                    return Err("byte stream descriptor and length must be integers");
                }
                let (fd, length) = (*fd, *length);
                let buffer = self.rewindBefore(inst).ins().uextend(I64, buffer);
                self.stringCall(inst, external, &[fd, buffer, length], false)
            }
            (SYM_STR_FROM_BYTES_UTF8, [data, length]) if self.source(*length).is_none() => {
                let ArrayData(array, ArrayElement::Scalar(Scalar::I8)) =
                    self.source(*data).ok_or("UTF-8 construction requires proven byte array storage")?
                else {
                    return Err("UTF-8 construction requires proven byte array storage");
                };
                let length = *length;
                let array = self.rewindBefore(inst).ins().uextend(I64, array);
                self.stringCall(inst, external, &[array, length], true)
            }
            (SYM_STR_FROM_I64, [value]) if self.source(*value).is_none() => {
                self.stringCall(inst, external, &[*value], true)
            }
            (SYM_STR_CONCAT | SYM_STR_EQ | SYM_STR_LEN | SYM_STR_SLICE | "str_cmp", args) => {
                let args = args.to_vec();
                let mut translated = Vec::with_capacity(args.len());
                for (index, value) in args.into_iter().enumerate() {
                    let expectsString = index == 0 || name != SYM_STR_SLICE;
                    match (expectsString, self.source(value)) {
                        (true, Some(String(handle))) => {
                            translated.push(self.rewindBefore(inst).ins().uextend(I64, handle))
                        }
                        (false, None) => translated.push(value),
                        _ => return Err("unproven string operand"),
                    }
                }
                self.stringCall(inst, external, &translated, matches!(name.as_str(), SYM_STR_CONCAT | SYM_STR_SLICE))
            }
            (text_services::FORMAT_F64, [value]) if self.source(*value).is_none() => {
                self.stringCall(inst, external, &[*value], true)
            }
            (text_services::PANIC_CODE, [code]) if self.source(*code).is_none() => {
                self.stringCall(inst, external, &[*code], false)
            }
            (host, args) if beskid_abi::runtime_source::browser_protocol::operation(host).is_some() => {
                use beskid_abi::runtime_source::browser_protocol::Value as Wire;
                let operation = beskid_abi::runtime_source::browser_protocol::operation(host).unwrap();
                let mut translated = Vec::with_capacity(args.len());
                for (argument, kind) in args.to_vec().into_iter().zip(operation.parameters) {
                    match (kind, self.source(argument)) {
                        (Wire::String, Some(String(handle))) | (Wire::Bytes, Some(Array(handle, ArrayElement::Scalar(Scalar::I8)))) =>
                            translated.push(self.rewindBefore(inst).ins().uextend(I64, handle)),
                        (Wire::Integer, None) => translated.push(argument),
                        _ => return Err("invalid argument to browser protocol service"),
                    }
                }
                self.stringCall(inst, external, &translated, operation.result == Wire::String)
            }
            (host, args) if crate::foundation::service(host).is_some() => {
                let service = crate::foundation::service(host).unwrap();
                let mut translated = Vec::with_capacity(args.len());
                for (argument, kind) in args.to_vec().into_iter().zip(service.args) {
                    match (kind, self.source(argument)) {
                        (beskid_abi::runtime_source::CorelibServiceAbiType::Pointer, Some(String(handle))) =>
                            translated.push(self.rewindBefore(inst).ins().uextend(I64, handle)),
                        (beskid_abi::runtime_source::CorelibServiceAbiType::I64, None) => translated.push(argument),
                        _ => return Err("invalid argument to Foundation browser service"),
                    }
                }
                self.stringCall(inst, external, &translated, service.returns_string())
            }
            (host, args) if text_services::reads_string(host) => {
                let (value, scalarPrefix) = args.split_last().ok_or("missing host string argument")?;
                let String(handle) = self.source(*value).ok_or("unproven host string argument")? else {
                    return Err("unproven host string argument");
                };
                if scalarPrefix.iter().any(|arg| self.source(*arg).is_some()) {
                    return Err("managed host scalar argument");
                }
                let mut translated = scalarPrefix.to_vec();
                translated.push(self.rewindBefore(inst).ins().uextend(I64, handle));
                self.stringCall(inst, external, &translated, false)
            }
            (host, args) if let Some(service) = crate::composition::service(host) => {
                let mut translated = args.to_vec();
                for (index, argument) in translated.iter_mut().enumerate() {
                    match self.source(*argument) {
                        Some(Packet(handle, _)) if service.managed.contains(&index) => {
                            *argument = self.rewindBefore(inst).ins().uextend(I64, handle);
                        }
                        None if !service.managed.contains(&index) || self.constant(*argument) == Some(0) => {}
                        _ => return Err("composition service argument disagrees with its managed source type"),
                    }
                }
                self.stringCall(inst, external, &translated, false)
            }
            (host, args) if self.scalarHosts.contains(host) => {
                if args.iter().any(|arg| self.source(*arg).is_some()) {
                    return Err("managed host scalar argument");
                }
                self.stringCall(inst, external, &args.to_vec(), false)
            }
            (pollName, []) if pollName == POLL || crate::is_clock_import(pollName) => {
                let callee = if let Some(callee) = self.imports.get(pollName) {
                    *callee
                } else {
                    let callee = self.function.import_function(external);
                    self.imports.insert(pollName.to_owned(), callee);
                    callee
                };
                let poll = self.rewindBefore(inst).ins().call(callee, &[]);
                // Reinserting the call removes its native stack-map annotation.
                self.function.dfg.replace_with_aliases(inst, poll);
                Ok(Edit::Erase)
            }
            (calleeName, args) if calleeName != ARRAY_ALLOC && calleeName != PACKET_ALLOC && calleeName != CLOSURE_ALLOC && calleeName != POLL => {
                let (params, result) = self.signatures.get(calleeName).ok_or("unknown user signature")?.clone();
                if params.len() != args.len() {
                    return Err("call argument count mismatch");
                }
                let args = args.to_vec();
                let mut translated = Vec::with_capacity(args.len());
                for (value, expected) in args.into_iter().zip(params) {
                    match (expected, self.source(value)) {
                        (Some(kind), Some(source)) => {
                            let handle = kind.handle(source).ok_or("managed argument type mismatch")?;
                            translated.push(self.rewindBefore(inst).ins().uextend(I64, handle))
                        }
                        (None, None) => translated.push(value),
                        _ => return Err("unproven managed call argument"),
                    }
                }
                let callee = self.function.import_function(external);
                let call = self.rewindBefore(inst).ins().call(callee, &translated);
                if let Some(record) = result {
                    let value = self.function.dfg.first_result(call);
                    let handle = self.rewindBefore(inst).ins().ireduce(I32, value);
                    self.eraseResult(inst, record.bind(handle))
                } else {
                    self.function.dfg.replace_with_aliases(inst, call);
                    Ok(Edit::Erase)
                }
            }
            _ => Err("unexpected native allocation, GC poll or managed argument"),
        }
    }

    fn stringCall(
        &mut self,
        inst: Inst,
        external: ExtFuncData,
        args: &[Value],
        returnsString: bool,
    ) -> Result<Edit, &'static str> {
        let callee = self.function.import_function(external);
        let call = self.rewindBefore(inst).ins().call(callee, args);
        if returnsString {
            let value = self.function.dfg.first_result(call);
            let handle = self.rewindBefore(inst).ins().ireduce(I32, value);
            self.eraseResult(inst, Native::String(handle))
        } else {
            self.function.dfg.replace_with_aliases(inst, call);
            Ok(Edit::Erase)
        }
    }

    fn rewriteIndirect(&mut self, inst: Inst, sig_ref: cranelift_codegen::ir::SigRef) -> Result<Edit, &'static str> {
        let values = self.function.dfg.inst_args(inst).to_vec();
        let (&callee, args) = values.split_first().ok_or("indirect call has no target")?;
        if let Some(slot) = self.dispatch.slots.get(&callee).copied() { return self.dispatch_contract(inst, slot, sig_ref, callee, args); }
        if !self.callPointers.contains(&self.function.dfg.resolve_aliases(callee)) {
            return Err("indirect call target is not a proven closure code field");
        }
        let shape = self.closureCalls.and_then(|calls| calls.get(&inst)).ok_or("indirect closure call has no source signature")?;
        let environment = args.first().and_then(|value| match self.source(*value) {
            Some(Native::Environment(handle)) => Some(handle),
            _ => None,
        });
        if !shape.matches(&self.function.dfg.signatures[sig_ref], environment.is_some()) {
            return Err("indirect call signature disagrees with source function type");
        }
        let mut translated = Vec::with_capacity(args.len());
        if let Some(handle) = environment {
            translated.push(self.rewindBefore(inst).ins().uextend(I64, handle));
        }
        for (&argument, &(_, expected)) in args[usize::from(environment.is_some())..].iter().zip(&shape.parameters) {
            let value = match (expected, self.source(argument)) {
                (Some(kind), Some(source)) => {
                    let handle = kind.handle(source).ok_or("managed closure argument type mismatch")?;
                    self.rewindBefore(inst).ins().uextend(I64, handle)
                }
                (None, None) => argument,
                _ => return Err("unproven managed closure argument"),
            };
            translated.push(value);
        }
        let call = self.rewindBefore(inst).ins().call_indirect(sig_ref, callee, &translated);
        if let Some((_, Some(kind))) = shape.result {
            let value = self.function.dfg.first_result(call);
            let handle = self.rewindBefore(inst).ins().ireduce(I32, value);
            self.eraseResult(inst, kind.bind(handle))
        } else {
            self.function.dfg.replace_with_aliases(inst, call);
            Ok(Edit::Erase)
        }
    }

    fn load(&mut self, inst: Inst, address: Value, offset: i32) -> Result<Edit, &'static str> {
        use Native::*;
        let result = self.function.dfg.first_result(inst);
        let ty = self.function.dfg.value_type(result);
        if ty == I64 && self.dispatch.pointers.contains(&address) {
            let pointer = self.rewindBefore(inst).ins().ireduce(I32, address);
            self.function.dfg.map_inst_values(inst, |arg| if arg == address { pointer } else { arg });
            return self.load_witness(inst, offset);
        }
        let (source, offset) = match self.source(address) {
            Some(PacketField(handle, record, fieldOffset)) if offset == 0 =>
                (Some(Packet(handle, record)), i32::try_from(fieldOffset).map_err(|_| "record field offset overflow")?),
            other => (other, offset),
        };
        match (source, offset, ty) {
            (Some(BarrierActive), 0, I64) if self.writeBarriers.probes.contains(&inst) => {
                self.function.replace(inst).iconst(I64, 0);
                Ok(Edit::Replace)
            }
            (Some(ArraySharing(handle, element)), 0, I64) | (Some(Array(handle, element)), -8, I64) => self.replaceCall(inst, &ArrayOp::Shared.symbol(element), &[handle], I64),
            (Some(ArrayDescriptor(plan)), 0 | 16 | 24, I64) => {
                let word = array_requests::word_index(offset).ok_or("invalid array request word")?;
                self.eraseResult(inst, StaticRequestWord { plan, word })
            }
            (Some(FiberSlot(slot, 8)), 0, I64) => {
                if let Some(value) = self.transferred_word(inst, slot) {
                    return self.eraseResult(inst, Received(value));
                }
                let block = self.function.layout.inst_block(inst).ok_or("fiber ABI load has no block")?;
                let handle = *self.joinSlots.get(&(slot, block)).ok_or("fiber ABI result has no dominating move")?;
                self.eraseResult(inst, Packet(handle, self.joinResult.ok_or("fiber result type missing")?))
            }
            (Some(Slot), 0, I64) => {
                let value = *self.slotValues.get(&inst)
                    .ok_or("spill slot load has no unique reaching definition")?;
                if let Some(kind) = self.source(value) {
                    self.eraseResult(inst, kind)
                } else {
                    let dfg = &mut self.function.dfg;
                    let result = dfg.first_result(inst);
                    dfg.clear_results(inst);
                    dfg.change_to_alias(result, value);
                    Ok(Edit::Erase)
                }
            }
            (Some(Received(value)), 16, ty) => self.load_transferred_value(inst, value, ty),
            (Some(Packet(_, record)), 0, I64) if self.hasTransferSlots && self.fields[record].len() == 1 =>
                self.eraseResult(inst, TransferDescriptor(record)),
            (Some(Packet(handle, record)), fieldOffset, I64) if Some(record) == self.dispatch.record => self.load_contract(inst, handle, record, fieldOffset),
            (Some(Packet(handle, record)), fieldOffset, _) => {
                let index = self.offsets[record]
                    .iter()
                    .position(|v| *v == i64::from(fieldOffset))
                    .ok_or("unknown record field")?;
                let field = self.fields[record][index];
                let symbol = RecordOp::Get(index).symbol(record);
                match field {
                    Field::Scalar(kind) if ty == scalar_type(kind) => {
                        if self.wrapperRecord == Some(record) && index == 0 {
                            self.callPointers.insert(result);
                        }
                        self.replaceCall(inst, &symbol, &[handle], ty)
                    }
                    Field::AnyRecord if ty == I64 && self.wrapperRecord == Some(record) => {
                        let env = self.callBefore(inst, &symbol, &[handle], Some(I32));
                        self.eraseResult(inst, Environment(self.function.dfg.first_result(env)))
                    }
                    Field::String if ty == I64 => {
                        let string = self.callBefore(inst, &symbol, &[handle], Some(I32));
                        self.eraseResult(inst, String(self.function.dfg.first_result(string)))
                    }
                    Field::Array(element) if ty == I64 => {
                        let array = self.callBefore(inst, &symbol, &[handle], Some(I32));
                        let array = self.function.dfg.first_result(array);
                        self.eraseResult(inst, Array(array, element))
                    }
                    Field::Record(fieldRecord) if ty == I64 => {
                        let field = self.callBefore(inst, &symbol, &[handle], Some(I32));
                        self.eraseResult(inst, Packet(self.function.dfg.first_result(field), fieldRecord))
                    }
                    _ => Err("record field type mismatch"),
                }
            }
            (Some(String(handle)), 0, I64) => self.eraseResult(inst, StringData(handle)),
            (Some(String(handle)), 8, I64) => {
                let handle = self.rewindBefore(inst).ins().uextend(I64, handle);
                self.replaceCall(inst, SYM_STR_LEN, &[handle], I64)
            }
            (Some(StringData(handle)), offset, ty)
                if offset >= 0 && matches!(ty, I8 | cranelift_codegen::ir::types::I16 | I32 | I64) =>
            {
                // Shared ABI-v5 CLIF compares short literals in little-endian chunks.
                // Each byte still reads from the proven GC string's backing array.
                let mut word = self.rewindBefore(inst).ins().iconst(I64, 0);
                for byte_index in 0..i64::from(ty.bytes()) {
                    let index = self.rewindBefore(inst).ins().iconst(I64, i64::from(offset) + byte_index);
                    let read = self.callBefore(inst, strings::BYTE_AT, &[handle, index], Some(I32));
                    let byte = self.function.dfg.first_result(read);
                    let mut cursor = self.rewindBefore(inst);
                    let byte = cursor.ins().uextend(I64, byte);
                    let shift = cursor.ins().iconst(I64, byte_index * 8);
                    let byte = cursor.ins().ishl(byte, shift);
                    word = cursor.ins().bor(word, byte);
                }
                if ty == I64 {
                    let replacement = self.function.dfg.value_def(word).unwrap_inst();
                    self.function.dfg.replace_with_aliases(inst, replacement);
                    Ok(Edit::Erase)
                } else {
                    self.function.replace(inst).ireduce(ty, word);
                    Ok(Edit::Replace)
                }
            }
            (Some(StringByte(handle, index)), 0, I8) => {
                let byte = self.callBefore(inst, strings::BYTE_AT, &[handle, index], Some(I32));
                let byte = self.function.dfg.first_result(byte);
                self.function.replace(inst).ireduce(I8, byte);
                Ok(Edit::Replace)
            }
            (Some(Array(handle, element)), 0, I64) => self.eraseResult(inst, ArrayData(handle, element)),
            (Some(Array(handle, element)), 8, I64) => self.arrayLength(inst, handle, element),
            (Some(Array(handle, element)), 16, I64) => {
                let capacity = self.callBefore(inst, &ArrayOp::Capacity.symbol(element), &[handle], Some(I32));
                let value = self.function.dfg.first_result(capacity);
                self.function.replace(inst).uextend(I64, value);
                Ok(Edit::Replace)
            }
            (Some(Element(handle, index, element)), 0, elementType) if elementType == native_element_type(element) => {
                let index = self.materializeIndex(inst, index);
                let helper = ArrayOp::Get.symbol(element);
                match element {
                    ArrayElement::Scalar(_) => self.replaceCall(inst, &helper, &[handle, index], elementType),
                    ArrayElement::String | ArrayElement::Record(_) | ArrayElement::Nested { .. } => {
                        let call = self.callBefore(inst, &helper, &[handle, index], Some(I32));
                        let field = self.function.dfg.first_result(call);
                        let field = match element {
                            ArrayElement::Record(record) => Packet(field, record),
                            ArrayElement::Nested { .. } => Array(field, element.inner().expect("nested array element")),
                            ArrayElement::String => String(field),
                            ArrayElement::Scalar(_) => unreachable!(),
                        };
                        self.eraseResult(inst, field)
                    }
                }
            }
            _ => Err("load is outside the supported record and array layout"),
        }
    }

    fn arrayLength(&mut self, inst: Inst, handle: Value, element: ArrayElement) -> Result<Edit, &'static str> {
        let len = self.callBefore(inst, &ArrayOp::Len.symbol(element), &[handle], Some(I32));
        let len = self.function.dfg.first_result(len);
        match self.function.dfg.insts[inst].opcode() {
            Opcode::Call => {
                let wide = self.rewindBefore(inst).ins().uextend(I64, len);
                let ValueDef::Result(replacement, _) = self.function.dfg.value_def(wide) else {
                    return Err("missing replaced array length");
                };
                self.function.dfg.replace_with_aliases(inst, replacement);
                Ok(Edit::Erase)
            }
            _ => {
                self.function.replace(inst).uextend(I64, len);
                Ok(Edit::Replace)
            }
        }
    }

    fn store(&mut self, inst: Inst, value: Value, destination: Option<Native>) -> Result<Edit, &'static str> {
        use Native::*;
        let source = self.source(value);
        let valueType = self.function.dfg.value_type(value);
        match (destination, source) {
            // A generic call result may be a scalar even when the native
            // stack-map pass spills its pointer-width representation.
            (Some(Slot), None | Some(Array(_, _) | Packet(_, _) | String(_) | Environment(_))) if valueType == I64 => {}
            (Some(ArraySharing(handle, element)), None) if valueType == I64 => {
                if !matches!(self.definition(value), Some(InstructionData::Binary { opcode: Opcode::Bor, args }) if args.iter().any(|arg| self.constant(*arg) == Some(1))) { return Err("unproven array sharing mark"); }
                self.callBefore(inst, &ArrayOp::MarkShared.symbol(element), &[handle], None);
            }
            (Some(RequestSlot(slot, word)), source) if valueType == I64 => {
                let data = match source {
                    Some(StaticRequestWord { plan, word }) => array_requests::Word::Static { plan, word },
                    None => array_requests::Word::Length(value),
                    _ => return Err("invalid native array request value"),
                };
                let block = self.function.layout.inst_block(inst).ok_or("array request store without a block")?;
                self.requests.store(block, slot, word, data)?;
            }
            (Some(FiberSlot(_, _)), None) if valueType == I64 && self.constant(value) == Some(0) => {},
            (Some(ArrayLength(handle, element)), None) if valueType == I64 => {
                let length = self.materializeIndex(inst, Index::Word(value));
                self.callBefore(inst, &ArrayOp::SetLen.symbol(element), &[handle, length], None);
            }
            (Some(PacketField(handle, record, offset)), _) if Some(record) == self.dispatch.record => return self.store_contract(inst, handle, record, offset, value),
            (Some(PacketField(handle, record, offset)), source) => {
                let index = self.offsets[record].iter().position(|v| *v == offset).ok_or("unknown record field")?;
                let field = self.fields[record][index];
                let value = match (field, source) {
                    (Field::Scalar(kind), None) if valueType == scalar_type(kind) => {
                        if self.wrapperRecord == Some(record) && index == 0 {
                            if !self.codePointers.contains(&value) { return Err("closure code is not a proven lambda entry"); }
                        }
                        value
                    }
                    (Field::AnyRecord, Some(Packet(environment, _))) if self.wrapperRecord == Some(record) => environment,
                    (Field::AnyRecord, None) if self.wrapperRecord == Some(record) && self.constant(value) == Some(0) =>
                        self.rewindBefore(inst).ins().iconst(I32, 0),
                    (Field::Array(expected), Some(Array(array, element))) if expected == element => array,
                    (Field::String, Some(String(string))) => string,
                    (Field::Record(expected), Some(Packet(field, actual))) if expected == actual => field,
                    _ => return Err("record field type mismatch"),
                };
                self.callBefore(inst, &RecordOp::Set(index).symbol(record), &[handle, value], None);
            }
            (Some(Element(handle, index, element)), source) => {
                let value = match (element, source) {
                    (ArrayElement::Scalar(kind), None) if valueType == scalar_type(kind) => value,
                    (ArrayElement::String | ArrayElement::Record(_) | ArrayElement::Nested { .. }, None)
                        if valueType == I64 && self.constant(value) == Some(0) =>
                    {
                        self.rewindBefore(inst).ins().iconst(I32, 0)
                    }
                    _ => array_managed_value(element, source).ok_or("array element store type mismatch")?,
                };
                let index = self.materializeIndex(inst, index);
                self.callBefore(inst, &ArrayOp::Set.symbol(element), &[handle, index, value], None);
            }
            _ => return Err("store is outside the supported record and array layout"),
        }
        Ok(Edit::Erase)
    }

    fn eraseResult(&mut self, inst: Inst, source: Native) -> Result<Edit, &'static str> {
        let result = self.function.dfg.first_result(inst);
        if self.function.dfg.value_type(result) != I64 {
            return Err("expected a native pointer-sized result");
        }
        self.managed.insert(result, source);
        Ok(Edit::Erase)
    }

    fn source(&self, value: Value) -> Option<Native> {
        self.managed.get(&self.function.dfg.resolve_aliases(value)).copied()
    }

    fn constant(&self, value: Value) -> Option<i64> {
        match self.definition(value)? {
            InstructionData::UnaryImm { opcode: Opcode::Iconst, imm } => Some(imm.bits()),
            _ => None,
        }
    }

    fn definition(&self, value: Value) -> Option<&InstructionData> {
        let dfg = &self.function.dfg;
        let ValueDef::Result(inst, _) = dfg.value_def(dfg.resolve_aliases(value)) else {
            return None;
        };
        Some(&dfg.insts[inst])
    }

    fn index(&self, offset: Value, element: ArrayElement) -> Option<Index> {
        let stride = i64::from(native_element_type(element).bytes());
        if let Some(offset) = self.constant(offset) {
            if offset % stride != 0 {
                return None;
            }
            return u32::try_from(offset / stride).ok().map(|index| Index::Constant(index as i32));
        }
        let index = if stride == 1 {
            offset
        } else {
            let InstructionData::Binary { opcode: Opcode::Imul, args: [index, scale] } = *self.definition(offset)?
            else {
                return None;
            };
            if self.constant(scale) != Some(stride) {
                return None;
            }
            index
        };
        match self.definition(index) {
            Some(InstructionData::Unary { opcode: Opcode::Uextend, arg })
                if self.function.dfg.value_type(*arg) == I32 =>
            {
                Some(Index::Scalar(*arg))
            }
            _ if self.function.dfg.value_type(index) == I64 && self.source(index).is_none() => Some(Index::Word(index)),
            _ => None,
        }
    }

    fn materializeIndex(&mut self, inst: Inst, index: Index) -> Value {
        match index {
            Index::Constant(value) => self.rewindBefore(inst).ins().iconst(I32, i64::from(value)),
            Index::Scalar(value) => value,
            Index::Word(value) => {
                use cranelift_codegen::ir::{TrapCode, condcodes::IntCC};
                let mut cursor = self.rewindBefore(inst);
                let tooLarge = cursor.ins().icmp_imm_u(IntCC::UnsignedGreaterThan, value, i64::from(u32::MAX));
                cursor.ins().trapnz(tooLarge, TrapCode::HEAP_OUT_OF_BOUNDS);
                cursor.ins().ireduce(I32, value)
            }
        }
    }

    fn callBefore(
        &mut self,
        inst: Inst,
        name: &str,
        args: &[Value],
        result: Option<cranelift_codegen::ir::Type>,
    ) -> Inst {
        let callee = self.helper(name, args, result);
        self.rewindBefore(inst).ins().call(callee, args)
    }

    fn replaceCall(
        &mut self,
        inst: Inst,
        name: &str,
        args: &[Value],
        result: cranelift_codegen::ir::Type,
    ) -> Result<Edit, &'static str> {
        let callee = self.helper(name, args, Some(result));
        self.function.replace(inst).call(callee, args);
        Ok(Edit::Replace)
    }

    fn helper(&mut self, name: &str, args: &[Value], result: Option<cranelift_codegen::ir::Type>) -> FuncRef {
        if let Some(callee) = self.imports.get(name) {
            return *callee;
        }
        let mut sig = Signature::new(self.function.signature.call_conv);
        sig.params.extend(args.iter().map(|value| AbiParam::new(self.function.dfg.value_type(*value))));
        if let Some(ty) = result {
            sig.returns.push(AbiParam::new(ty));
        }
        let signature = self.function.import_signature(sig);
        let reference = self.function.import_function(ExtFuncData {
            name: ExternalName::testcase(name),
            signature,
            colocated: false,
            patchable: false,
        });
        self.imports.insert(name.to_owned(), reference);
        reference
    }
}

fn scalar_type(kind: Scalar) -> cranelift_codegen::ir::Type {
    use cranelift_codegen::ir::types::{F32, F64};
    match kind {
        Scalar::I8 => I8,
        Scalar::I16 => cranelift_codegen::ir::types::I16,
        Scalar::I32 => I32,
        Scalar::I64 => I64,
        Scalar::F32 => F32,
        Scalar::F64 => F64,
    }
}
