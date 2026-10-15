//! Compiler-issued registered Dynamic mapping transforms.
//!
//! A mapping is issued only from an exact canonical `Mapping<S, D>` binding and the plan
//! that `beskid_queries::dynamic_mapping_plan` approves from the two canonical packing
//! graphs. Layouts come from the same `aggregate_object_layout_from_fact` and enum payload
//! layout authority that ordinary allocation uses. The transform is native code in the
//! sealed Dynamic initializer image closure, registered as roles 5 (transform) and 15
//! (checked transform) of that image. It builds a complete fresh destination box before
//! the runtime publishes anything: every intermediate destination object stays rooted
//! until its parent holds it, every failure unroots what it holds and returns null, and
//! the source is never written.
use crate::{AggregateObjectLayout, AggregateStaticPlan, CodegenInput};
use beskid_queries::{
    AggregateFieldShape, AggregateLayoutFact, DynamicMappingBinding, DynamicMappingOutcome, DynamicMappingPlan,
    DynamicMappingStep, DynamicPackingNode, DynamicPackingShape, EnumLayoutFact, EnumVariantLayoutFact,
    SemanticTypeId,
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::sync::Arc;

/// Runtime nesting bound of one transform. Cyclic source values fail closed at this depth.
const MAX_RUNTIME_DEPTH: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDynamicMapping {
    source_shape: [u8; 32],
    destination_shape: [u8; 32],
    approved: Option<ApprovedDynamicMapping>,
    rejection: Option<String>,
}
impl CompiledDynamicMapping {
    pub fn source_shape(&self) -> &[u8; 32] {
        &self.source_shape
    }
    pub fn destination_shape(&self) -> &[u8; 32] {
        &self.destination_shape
    }
    /// `None` when the plan was rejected; such a mapping registers nothing.
    pub fn approved(&self) -> Option<&ApprovedDynamicMapping> {
        self.approved.as_ref()
    }
    pub fn rejection(&self) -> Option<&str> {
        self.rejection.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedDynamicMapping {
    signature: Vec<u8>,
    digest: [u8; 32],
    native: String,
}
impl ApprovedDynamicMapping {
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn token_getter(&self) -> String {
        format!("beskid_dynamic_mapping_token_v1_{}", hex(&self.digest))
    }
    pub(crate) fn prefix(&self) -> String {
        prefix(&self.digest)
    }
    pub(crate) fn transform_symbol(&self) -> String {
        format!("{}_transform", self.prefix())
    }
    pub(crate) fn checked_transform_symbol(&self) -> String {
        format!("{}_checked_transform", self.prefix())
    }
    /// C definitions of this mapping's transforms. They reference the destination
    /// payload descriptor getter declared by the enclosing initializer.
    pub(crate) fn native_definitions(&self) -> &str {
        &self.native
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn prefix(digest: &[u8; 32]) -> String {
    format!("beskid_dynamic_map_v1_{}", hex(digest))
}

impl CodegenInput<'_> {
    /// Compile the mapping for one exact canonical binding. A rejected plan is a checked
    /// outcome; any missing source authority is an error.
    pub fn compiled_dynamic_mapping(&self, binding: &DynamicMappingBinding) -> Result<CompiledDynamicMapping, String> {
        let source = self
            .compiled_dynamic_packing_shape(binding.source())?
            .ok_or("Dynamic mapping source graph is unavailable")?;
        let destination = self
            .compiled_dynamic_packing_shape(binding.destination())?
            .ok_or("Dynamic mapping destination graph is unavailable")?;
        let outcome = beskid_queries::dynamic_mapping_plan(self.database(), binding).map_err(|error| error.to_string())?;
        let plan = match outcome {
            DynamicMappingOutcome::Rejected(reason) => {
                return Ok(CompiledDynamicMapping {
                    source_shape: *source.sha256(),
                    destination_shape: *destination.sha256(),
                    approved: None,
                    rejection: Some(reason),
                });
            }
            DynamicMappingOutcome::Approved(plan) => plan,
        };
        if self.target().pointer_width != 64 {
            return Err("Dynamic mapping requires a 64-bit target".into());
        }
        let source_box = self.dynamic_mapping_box(binding.source())?;
        let destination_box = self.dynamic_mapping_box(binding.destination())?;
        let destination_getter = destination_box
            .descriptor_getter
            .clone()
            .ok_or("Dynamic mapping destination descriptor getter is unavailable")?;
        let mut signature = b"beskid.Dynamic.Mapping.V1\0".to_vec();
        signature.extend_from_slice(source.sha256());
        signature.extend_from_slice(destination.sha256());
        signature.extend_from_slice(format!("{:?}", plan.steps()).as_bytes());
        if signature.len() > 1024 * 1024 {
            return Err("Dynamic mapping signature exceeds 1 MiB".into());
        }
        let digest: [u8; 32] = Sha256::digest(&signature).into();
        let native = Emitter { input: self, plan: &plan, prefix: prefix(&digest) }.emit(
            &source_box,
            &destination_box,
            &destination_getter,
        )?;
        Ok(CompiledDynamicMapping {
            source_shape: *source.sha256(),
            destination_shape: *destination.sha256(),
            approved: Some(ApprovedDynamicMapping { signature, digest, native }),
            rejection: None,
        })
    }

    /// The exact source-issued Pack box allocation of one packing instance.
    pub(crate) fn dynamic_mapping_box(
        &self,
        packing: &beskid_queries::GenericSpecializationInstance,
    ) -> Result<AggregateStaticPlan, String> {
        let bridge = beskid_queries::dynamic_packing_bridge(self.database(), packing)
            .map_err(|error| error.to_string())?
            .ok_or("Dynamic mapping lacks a canonical Pack box")?;
        let plan = self
            .aggregate_static_plan_for_specialization(bridge.box_literal(), Some(packing))
            .ok_or("Dynamic mapping Pack box layout is unavailable")?;
        if plan.fields.len() != 1 {
            return Err("canonical Pack box field count changed".into());
        }
        Ok(plan)
    }
}

/// C storage class of one scalar value.
fn c_scalar(name: &str) -> Option<(&'static str, u64)> {
    Some(match name {
        "i8" => ("int8_t", 1),
        "i16" => ("int16_t", 2),
        "i32" => ("int32_t", 4),
        "i64" => ("int64_t", 8),
        "u8" | "bool" => ("uint8_t", 1),
        "u16" => ("uint16_t", 2),
        "u32" | "char" | "f32" => ("uint32_t", 4),
        "u64" | "f64" | "word" => ("uint64_t", 8),
        _ => return None,
    })
}
fn semantic_scalar(name: &str) -> Option<SemanticTypeId> {
    Some(match name {
        "i8" => SemanticTypeId::I8,
        "i16" => SemanticTypeId::I16,
        "i32" => SemanticTypeId::I32,
        "i64" => SemanticTypeId::I64,
        "u8" => SemanticTypeId::U8,
        "u16" => SemanticTypeId::U16,
        "u32" => SemanticTypeId::U32,
        "u64" => SemanticTypeId::U64,
        "f32" => SemanticTypeId::F32,
        "f64" => SemanticTypeId::F64,
        "bool" => SemanticTypeId::BOOL,
        "char" => SemanticTypeId::CHAR,
        "word" => SemanticTypeId::WORD,
        "utf8" => SemanticTypeId::STRING,
        "unit" => SemanticTypeId::UNIT,
        _ => return None,
    })
}

/// Where a mapped value is stored: a rooted record or box (`target`, reloaded from its
/// root after every allocation) or an element of a rooted array under construction.
/// Collection is stop-the-world, so record stores need no barrier, as in generated
/// code; array element stores use the canonical array construction barrier.
struct Slot {
    reload: String,
    address: String,
    barrier: &'static str,
}
impl Slot {
    fn barrier(&self, value: &str) -> String {
        if self.barrier.is_empty() { String::new() } else { format!("{}{value});", self.barrier) }
    }
}

struct Emitter<'a, 'db> {
    input: &'a CodegenInput<'db>,
    plan: &'a DynamicMappingPlan,
    prefix: String,
}

impl Emitter<'_, '_> {
    fn node(graph: &DynamicPackingShape, index: u32) -> Result<&DynamicPackingNode, String> {
        graph.nodes().get(index as usize).ok_or_else(|| "Dynamic mapping node outside graph".to_owned())
    }

    fn field_shape(graph: &DynamicPackingShape, ty: u32) -> Result<AggregateFieldShape, String> {
        Ok(match Self::node(graph, ty)? {
            DynamicPackingNode::Scalar { name, .. } => AggregateFieldShape::Scalar(
                semantic_scalar(name).ok_or_else(|| format!("Dynamic mapping scalar {name} has no layout"))?,
            ),
            DynamicPackingNode::Array { .. } => AggregateFieldShape::ManagedReference(SemanticTypeId::POINTER),
            DynamicPackingNode::Record { declaration, .. } | DynamicPackingNode::Enum { declaration, .. } => {
                AggregateFieldShape::Nominal(*declaration)
            }
        })
    }

    /// Record layout from the canonical layout authority. A non-generic declaration is
    /// cross-checked against its own declaration layout.
    fn record_layout(&self, graph: &DynamicPackingShape, node: u32) -> Result<AggregateObjectLayout, String> {
        let DynamicPackingNode::Record { declaration, arguments, fields, .. } = Self::node(graph, node)? else {
            return Err("Dynamic mapping record layout requested for a non-record".into());
        };
        let fact = AggregateLayoutFact {
            fields: fields
                .iter()
                .map(|field| Ok((Arc::<str>::from(field.name.as_str()), Self::field_shape(graph, field.ty)?)))
                .collect::<Result<Vec<_>, String>>()?
                .into(),
        };
        let layout = self
            .input
            .aggregate_object_layout_from_fact(*declaration, &fact)
            .ok_or("Dynamic mapping record layout is unavailable")?;
        if layout.fields.len() != fields.len() || !layout.injected_fields.is_empty() {
            return Err("Dynamic mapping record layout disagrees with its packing graph".into());
        }
        if arguments.is_empty() {
            if let Some(canonical) = self.input.aggregate_object_layout(*declaration) {
                if canonical.object_size != layout.object_size
                    || canonical.object_alignment != layout.object_alignment
                    || canonical.pointer_map_offsets != layout.pointer_map_offsets
                    || canonical.fields.iter().map(|field| field.field_offset).ne(layout.fields.iter().map(|field| field.field_offset))
                {
                    return Err("Dynamic mapping record layout disagrees with its declaration layout".into());
                }
            }
        }
        Ok(layout)
    }

    /// Element storage of an array whose element node is `element`.
    fn element(graph: &DynamicPackingShape, element: u32) -> Result<(u64, u64, bool), String> {
        Ok(match Self::node(graph, element)? {
            DynamicPackingNode::Scalar { name, .. } => {
                let layout = semantic_scalar(name)
                    .and_then(|ty| ty.scalar_abi_layout(64))
                    .ok_or_else(|| format!("Dynamic mapping array element {name} has no storage"))?;
                (layout.size, layout.alignment, layout.is_pointer)
            }
            _ => (8, 8, true),
        })
    }

    fn pointer_map(name: &str, offsets: &[u64]) -> String {
        let values = if offsets.is_empty() {
            "0".to_owned()
        } else {
            offsets.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
        };
        format!("static const uint64_t {name}[{}]={{{values}}};\n", offsets.len().max(1))
    }

    fn function(&self, step: u32) -> String {
        format!("{}_s{step}", self.prefix)
    }

    /// One mapped value transfer from `source` (a C `const uint8_t *` slot address, absent
    /// for a declared optional value) into `slot`.
    fn transfer(&self, step: u32, source: Option<&str>, slot: &Slot) -> Result<String, String> {
        let step_value = self.plan.steps().get(step as usize).ok_or("Dynamic mapping step outside plan")?;
        let Slot { reload, address, .. } = slot;
        let (barrier_value, barrier_mapped) = (slot.barrier("value"), slot.barrier("mapped"));
        let load = |source: Option<&str>| -> Result<String, String> {
            source
                .map(|source| format!("memcpy(&value,{source},8);"))
                .ok_or_else(|| "Dynamic mapping source slot is missing".to_owned())
        };
        Ok(match step_value {
            DynamicMappingStep::Exact { scalar: "unit" } => String::new(),
            DynamicMappingStep::Exact { scalar: "utf8" } | DynamicMappingStep::Shared { .. } => {
                let load = load(source)?;
                format!(
                    "{{void *value=NULL;{load}if(!value)goto failed;{reload}memcpy({address},&value,8);{barrier_value}}}\n"
                )
            }
            DynamicMappingStep::Exact { scalar } => {
                let (_, size) = c_scalar(scalar).ok_or_else(|| format!("Dynamic mapping scalar {scalar} has no storage"))?;
                let source = source.ok_or("Dynamic mapping scalar source slot is missing")?;
                format!("{reload}memcpy({address},{source},{size});\n")
            }
            DynamicMappingStep::Widen { source: from, destination: to } => {
                let (from_type, from_size) = c_scalar(from).ok_or("Dynamic mapping widening source has no storage")?;
                let (to_type, to_size) = c_scalar(to).ok_or("Dynamic mapping widening destination has no storage")?;
                let source = source.ok_or("Dynamic mapping widening source slot is missing")?;
                format!(
                    "{reload}{{{from_type} narrow=0;memcpy(&narrow,{source},{from_size});{to_type} wide=({to_type})narrow;memcpy({address},&wide,{to_size});}}\n"
                )
            }
            DynamicMappingStep::OptionalNone { .. } => {
                let function = self.function(step);
                format!(
                    "{{void *mapped={function}(NULL,checked,depth+1);if(!mapped)goto failed;{reload}memcpy({address},&mapped,8);{barrier_mapped}}}\n"
                )
            }
            DynamicMappingStep::Array { .. } | DynamicMappingStep::Record { .. } => {
                let load = load(source)?;
                let function = self.function(step);
                format!(
                    "{{void *value=NULL;{load}void *mapped={function}((const uint8_t *)value,checked,depth+1);if(!mapped)goto failed;{reload}memcpy({address},&mapped,8);{barrier_mapped}}}\n"
                )
            }
        })
    }

    fn record_slot(offset: u64) -> Slot {
        Slot {
            reload: "target=(uint8_t *)beskid_rt_v5_gc_resolve_handle(root);if(!target)goto failed;".into(),
            address: format!("target+{offset}"),
            barrier: "",
        }
    }

    fn emit(&self, source_box: &AggregateStaticPlan, destination_box: &AggregateStaticPlan, getter: &str) -> Result<String, String> {
        let prefix = &self.prefix;
        let source = self.plan.source();
        let destination = self.plan.destination();
        let mut declarations = String::new();
        let mut definitions = String::new();
        for (index, step) in self.plan.steps().iter().enumerate() {
            let index = index as u32;
            let function = self.function(index);
            match step {
                DynamicMappingStep::Record { source: from, destination: to, fields } => {
                    let source_layout = self.record_layout(source, *from)?;
                    let destination_layout = self.record_layout(destination, *to)?;
                    let DynamicPackingNode::Record { fields: source_fields, .. } = Self::node(source, *from)? else {
                        return Err("Dynamic mapping record source changed".into());
                    };
                    let DynamicPackingNode::Record { fields: destination_fields, .. } = Self::node(destination, *to)?
                    else {
                        return Err("Dynamic mapping record destination changed".into());
                    };
                    definitions.push_str(&Self::pointer_map(&format!("{prefix}_m{index}"), &destination_layout.pointer_map_offsets));
                    writeln!(
                        definitions,
                        "static const BeskidDynamicMapTypeDescriptorV1 {prefix}_d{index}={{{},{},{prefix}_m{index},{},0}};",
                        destination_layout.object_size,
                        destination_layout.object_alignment,
                        destination_layout.pointer_map_offsets.len()
                    )
                    .map_err(|error| error.to_string())?;
                    writeln!(
                        definitions,
                        "static const BeskidDynamicMapRequestV1 {prefix}_q{index}={{{},{},&{prefix}_d{index}}};",
                        destination_layout.object_size, destination_layout.object_alignment
                    )
                    .map_err(|error| error.to_string())?;
                    writeln!(declarations, "static void *{function}(const uint8_t *,int,unsigned);")
                        .map_err(|error| error.to_string())?;
                    let mut body = String::new();
                    if fields.len() != destination_fields.len() {
                        return Err("Dynamic mapping record plan does not cover every destination field".into());
                    }
                    for field in fields {
                        let destination_offset = destination_layout
                            .fields
                            .get(field.destination)
                            .ok_or("Dynamic mapping destination field outside layout")?
                            .field_offset;
                        let source_slot = match field.source {
                            Some(position) => {
                                if position >= source_fields.len() {
                                    return Err("Dynamic mapping source field outside graph".into());
                                }
                                let offset = source_layout
                                    .fields
                                    .get(position)
                                    .ok_or("Dynamic mapping source field outside layout")?
                                    .field_offset;
                                Some(format!("source+{offset}"))
                            }
                            None => None,
                        };
                        body.push_str(&self.transfer(field.step, source_slot.as_deref(), &Self::record_slot(destination_offset))?);
                    }
                    write!(
                        definitions,
                        "static void *{function}(const uint8_t *source,int checked,unsigned depth){{\n\
                         if(!source || depth>{MAX_RUNTIME_DEPTH}) return NULL;\n\
                         uint8_t *target=(uint8_t *)beskid_dynamic_map_v1_allocate(&{prefix}_q{index},checked);\n\
                         size_t root=beskid_dynamic_map_v1_root(target,checked);\n\
                         if(!root) return NULL;\n\
                         {body}\
                         target=(uint8_t *)beskid_rt_v5_gc_resolve_handle(root);\n\
                         gc_unroot_handle(root);\n\
                         return target;\n\
                         goto failed;\n\
                         failed:\n\
                         gc_unroot_handle(root);\n\
                         return NULL;\n\
                         }}\n"
                    )
                    .map_err(|error| error.to_string())?;
                }
                DynamicMappingStep::Array { source: from, destination: to, element } => {
                    let (DynamicPackingNode::Array { element: source_element }, DynamicPackingNode::Array { element: destination_element }) =
                        (Self::node(source, *from)?, Self::node(destination, *to)?)
                    else {
                        return Err("Dynamic mapping array node changed".into());
                    };
                    let (source_stride, _, _) = Self::element(source, *source_element)?;
                    let (stride, alignment, pointer) = Self::element(destination, *destination_element)?;
                    if pointer {
                        definitions.push_str(&Self::pointer_map(&format!("{prefix}_m{index}"), &[0]));
                    }
                    writeln!(
                        definitions,
                        "static const BeskidDynamicMapElementDescriptorV1 {prefix}_e{index}={{{stride},{alignment},{},{}}};",
                        if pointer { format!("{prefix}_m{index}") } else { "NULL".to_owned() },
                        u64::from(pointer)
                    )
                    .map_err(|error| error.to_string())?;
                    writeln!(declarations, "static void *{function}(const uint8_t *,int,unsigned);")
                        .map_err(|error| error.to_string())?;
                    let slot = Slot {
                        reload: "memcpy(&elements,array,8);".into(),
                        address: format!("elements+index*{stride}"),
                        barrier: "beskid_rt_v5_array_write_barrier(array,",
                    };
                    let body = self.transfer(*element, Some(&format!("data+index*{source_stride}")), &slot)?;
                    write!(
                        definitions,
                        "static void *{function}(const uint8_t *source,int checked,unsigned depth){{\n\
                         if(!source || depth>{MAX_RUNTIME_DEPTH}) return NULL;\n\
                         const uint8_t *data=NULL;uint64_t length=0;\n\
                         memcpy(&data,source,8);memcpy(&length,source+8,8);\n\
                         if((length && !data) || length>UINT64_MAX/{stride}) return NULL;\n\
                         BeskidDynamicMapArrayRequestV1 request={{&{prefix}_e{index},length,&beskid_dynamic_map_v1_array_descriptor,0}};\n\
                         size_t root=0;\n\
                         uint8_t *array=(uint8_t *)(checked ? beskid_rt_v5_array_try_allocate_rooted(&request,&root)\n\
                             : beskid_rt_v5_array_allocate_rooted(&request,&root));\n\
                         if(!array || !root || (checked && beskid_rt_v5_checked_scope_failure_reason())) {{\n\
                         if(root) beskid_rt_v5_array_construction_finish(root);\n\
                         return NULL;\n\
                         }}\n\
                         uint8_t *elements=NULL;\n\
                         for(uint64_t index=0;index<length;index++) {{\n\
                         {body}\
                         }}\n\
                         (void)elements;\n\
                         beskid_rt_v5_array_construction_finish(root);\n\
                         return array;\n\
                         goto failed;\n\
                         failed:\n\
                         beskid_rt_v5_array_construction_finish(root);\n\
                         return NULL;\n\
                         }}\n"
                    )
                    .map_err(|error| error.to_string())?;
                }
                DynamicMappingStep::OptionalNone { destination: node, ordinal } => {
                    let DynamicPackingNode::Enum { variants, .. } = Self::node(destination, *node)? else {
                        return Err("Dynamic mapping optional node changed".into());
                    };
                    let fact = EnumLayoutFact {
                        variants: variants
                            .iter()
                            .map(|variant| {
                                Ok(EnumVariantLayoutFact {
                                    name: Arc::from(variant.name.as_str()),
                                    fields: variant
                                        .fields
                                        .iter()
                                        .map(|field| Ok((Arc::<str>::from(field.name.as_str()), Self::field_shape(destination, field.ty)?)))
                                        .collect::<Result<Vec<_>, String>>()?
                                        .into(),
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()?
                            .into(),
                    };
                    let header = self
                        .input
                        .abi_manifest()
                        .layouts
                        .iter()
                        .find(|layout| layout.name == "BeskidObjectHeader")
                        .ok_or("Dynamic mapping object header layout is unavailable")?;
                    let physical = fact
                        .scalar_payload_object_layout(64, header.size, header.alignment)
                        .ok_or("Dynamic mapping optional layout is unavailable")?;
                    definitions.push_str(&Self::pointer_map(&format!("{prefix}_m{index}"), &physical.pointer_map_offsets));
                    writeln!(
                        definitions,
                        "static const BeskidDynamicMapTypeDescriptorV1 {prefix}_d{index}={{{},{},{prefix}_m{index},{},0}};",
                        physical.object_size,
                        physical.object_alignment,
                        physical.pointer_map_offsets.len()
                    )
                    .map_err(|error| error.to_string())?;
                    writeln!(
                        definitions,
                        "static const BeskidDynamicMapRequestV1 {prefix}_q{index}={{{},{},&{prefix}_d{index}}};",
                        physical.object_size, physical.object_alignment
                    )
                    .map_err(|error| error.to_string())?;
                    writeln!(declarations, "static void *{function}(const uint8_t *,int,unsigned);")
                        .map_err(|error| error.to_string())?;
                    write!(
                        definitions,
                        "static void *{function}(const uint8_t *source,int checked,unsigned depth){{\n\
                         (void)source;(void)depth;\n\
                         uint8_t *target=(uint8_t *)beskid_dynamic_map_v1_allocate(&{prefix}_q{index},checked);\n\
                         if(!target) return NULL;\n\
                         int32_t tag={ordinal};memcpy(target+{},&tag,4);\n\
                         return target;\n\
                         }}\n",
                        physical.tag_offset
                    )
                    .map_err(|error| error.to_string())?;
                }
                DynamicMappingStep::Exact { .. } | DynamicMappingStep::Widen { .. } | DynamicMappingStep::Shared { .. } => {}
            }
        }
        let source_offset = source_box.fields[0].field_offset;
        let destination_offset = destination_box.fields[0].field_offset;
        let root = self.transfer(0, Some(&format!("source+{source_offset}")), &Self::record_slot(destination_offset))?;
        let mut native = declarations;
        native.push_str(&definitions);
        write!(
            native,
            "static void *{prefix}_box(void *source_box,int checked){{\n\
             if(!source_box) return NULL;\n\
             const uint8_t *source=(const uint8_t *)source_box;\n\
             unsigned depth=0;\n\
             BeskidDynamicMapRequestV1 request={{{},{},{getter}()}};\n\
             if(!request.descriptor) return NULL;\n\
             uint8_t *target=(uint8_t *)beskid_dynamic_map_v1_allocate(&request,checked);\n\
             size_t root=beskid_dynamic_map_v1_root(target,checked);\n\
             if(!root) return NULL;\n\
             {root}\
             (void)depth;\n\
             target=(uint8_t *)beskid_rt_v5_gc_resolve_handle(root);\n\
             gc_unroot_handle(root);\n\
             return target;\n\
             goto failed;\n\
             failed:\n\
             gc_unroot_handle(root);\n\
             return NULL;\n\
             }}\n\
             static void *{prefix}_transform(void *source_box){{return {prefix}_box(source_box,0);}}\n\
             static void *{prefix}_checked_transform(void *source_box){{return {prefix}_box(source_box,1);}}\n",
            destination_box.object_size, destination_box.object_alignment
        )
        .map_err(|error| error.to_string())?;
        Ok(native)
    }
}

/// Shared C support for every mapping transform of one initializer image. Allocation,
/// rooting and barriers are the canonical runtime exports; checked transforms use the
/// guarded variants and inspect the sticky checked failure before reading any result.
pub(crate) fn native_support() -> String {
    format!(
        "typedef struct {{uint64_t size;uint64_t alignment;const void *map;uint64_t count;uint64_t flags;}} BeskidDynamicMapTypeDescriptorV1;\n\
         typedef struct {{uint64_t size;uint64_t alignment;const void *descriptor;}} BeskidDynamicMapRequestV1;\n\
         typedef struct {{uint64_t stride;uint64_t alignment;const void *map;uint64_t count;}} BeskidDynamicMapElementDescriptorV1;\n\
         typedef struct {{const void *element;uint64_t length;const void *descriptor;uint64_t reserved;}} BeskidDynamicMapArrayRequestV1;\n\
         _Static_assert(sizeof(BeskidDynamicMapTypeDescriptorV1)==40,\"Dynamic mapping descriptor\");\n\
         _Static_assert(sizeof(BeskidDynamicMapRequestV1)==24,\"Dynamic mapping request\");\n\
         _Static_assert(sizeof(BeskidDynamicMapElementDescriptorV1)==32,\"Dynamic mapping element descriptor\");\n\
         _Static_assert(sizeof(BeskidDynamicMapArrayRequestV1)==32,\"Dynamic mapping array request\");\n\
         static const BeskidDynamicMapTypeDescriptorV1 beskid_dynamic_map_v1_array_descriptor={{48,8,NULL,0,1}};\n\
         extern void *beskid_rt_v5_managed_object_allocate(const void *);\n\
         extern void *beskid_rt_v5_managed_object_try_allocate(const void *);\n\
         extern void *beskid_rt_v5_array_allocate_rooted(const void *,size_t *);\n\
         extern void *beskid_rt_v5_array_try_allocate_rooted(const void *,size_t *);\n\
         extern uint8_t beskid_rt_v5_array_construction_finish(size_t);\n\
         extern uint8_t beskid_rt_v5_array_write_barrier(void *,void *);\n\
         extern size_t gc_root_handle(void *);\n\
         extern size_t beskid_rt_v5_gc_try_root_handle(void *);\n\
         extern void gc_unroot_handle(size_t);\n\
         extern void *beskid_rt_v5_gc_resolve_handle(size_t);\n\
         extern size_t beskid_rt_v5_checked_scope_failure_reason(void);\n\
         static void *beskid_dynamic_map_v1_allocate(const BeskidDynamicMapRequestV1 *request,int checked){{\n\
         void *object=checked ? beskid_rt_v5_managed_object_try_allocate(request) : beskid_rt_v5_managed_object_allocate(request);\n\
         if(!object || (checked && beskid_rt_v5_checked_scope_failure_reason())) return NULL;\n\
         return object;\n\
         }}\n\
         static size_t beskid_dynamic_map_v1_root(void *object,int checked){{\n\
         if(!object) return 0;\n\
         size_t root=checked ? beskid_rt_v5_gc_try_root_handle(object) : gc_root_handle(object);\n\
         if(root && checked && beskid_rt_v5_checked_scope_failure_reason()) {{gc_unroot_handle(root);return 0;}}\n\
         return root;\n\
         }}\n"
    )
}
