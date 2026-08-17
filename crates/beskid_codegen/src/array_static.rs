//! ABI-v5 static metadata for descriptor-backed managed array literals.
//!
//! Arrays deliberately do not reuse the generic `array_new(element_size, length)` runtime path:
//! size contains no pointer-map information and would make managed elements invisible after a
//! later collection.  This module emits immutable element metadata and one request per syntax
//! literal, all derived from current-generation semantic facts.

use std::sync::Arc;

use beskid_queries::{
    AstNodeKey, CallLowering, IndexedNodeKind, SemanticTypeId, bulk_parameter, call_arguments, call_lowering,
    child_nodes, empty_array_literal_element_abi_type, node_kind, node_type,
};
use cranelift_module::{DataDescription, DataId, Linkage, Module, ModuleError, ModuleResult};

use crate::{CodegenInput, aggregate_static::paths_match};

pub const ABI_V5_ARRAY_ALLOCATE_ROOTED: &str = "beskid_rt_v5_array_allocate_rooted";
pub const ABI_V5_ARRAY_GROW_ROOTED: &str = "beskid_rt_v5_array_grow_rooted";
pub const ABI_V5_ARRAY_CONSTRUCTION_FINISH: &str = "beskid_rt_v5_array_construction_finish";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayStaticPlan {
    pub literal: AstNodeKey,
    pub pointer_map_symbol: String,
    pub element_descriptor_symbol: String,
    pub allocation_request_symbol: String,
    pub element_type: SemanticTypeId,
    pub stride: u64,
    pub alignment: u64,
    pub pointer_map_offsets: Arc<[u64]>,
    pub length: u64,
}

pub fn emit_array_static_data<M: Module>(
    module: &mut M,
    plan: &ArrayStaticPlan,
) -> ModuleResult<(DataId, DataId, DataId)> {
    let pointer_map = module.declare_data(&plan.pointer_map_symbol, Linkage::Local, false, false)?;
    let element = module.declare_data(&plan.element_descriptor_symbol, Linkage::Local, false, false)?;
    let request = module.declare_data(&plan.allocation_request_symbol, Linkage::Local, false, false)?;

    let mut pointer_map_bytes = Vec::with_capacity(plan.pointer_map_offsets.len().max(1) * 8);
    if plan.pointer_map_offsets.is_empty() {
        // Keep the unused local data object non-empty for object-module backends;
        // the descriptor itself deliberately retains a null map pointer below.
        pointer_map_bytes.extend_from_slice(&0u64.to_le_bytes());
    } else {
        for offset in plan.pointer_map_offsets.iter().copied() {
            pointer_map_bytes.extend_from_slice(&offset.to_le_bytes());
        }
    }
    let mut pointer_map_data = DataDescription::new();
    pointer_map_data.define(pointer_map_bytes.into_boxed_slice());
    // Pointer-map entries are native words read by the runtime.  `define` does not imply an ABI
    // alignment, so make the object constraint explicit for every target object backend.
    pointer_map_data.set_align(8);
    module.define_data(pointer_map, &pointer_map_data)?;

    // BeskidArrayElementDescriptor { stride, alignment, pointer_map, pointer_count }.
    let mut element_bytes = vec![0u8; 32];
    write_word(&mut element_bytes, 0, plan.stride)?;
    write_word(&mut element_bytes, 8, plan.alignment)?;
    write_word(&mut element_bytes, 24, plan.pointer_map_offsets.len() as u64)?;
    let mut element_data = DataDescription::new();
    element_data.define(element_bytes.into_boxed_slice());
    // Manifest freezes BeskidArrayElementDescriptor at 8-byte alignment.
    element_data.set_align(8);
    // The ABI contract requires a null map pointer when the count is zero; a
    // placeholder data object's address would be malformed metadata even though
    // it is never scanned.
    if !plan.pointer_map_offsets.is_empty() {
        let pointer_map_address = module.declare_data_in_data(pointer_map, &mut element_data);
        element_data.write_data_addr(16, pointer_map_address, 0);
    }
    module.define_data(element, &element_data)?;

    // BeskidArrayAllocationRequest followed by its immutable array-object descriptor. Keeping the
    // fixed descriptor in the same data object gives each request a valid, process-lifetime
    // descriptor without conflating it with the separately addressable element descriptor.
    let mut request_bytes = vec![0u8; 72];
    write_word(&mut request_bytes, 8, plan.length)?;
    write_word(&mut request_bytes, 32, 48)?;
    write_word(&mut request_bytes, 40, 8)?;
    write_word(&mut request_bytes, 64, 1)?;
    let mut request_data = DataDescription::new();
    request_data.define(request_bytes.into_boxed_slice());
    // Manifest freezes BeskidArrayAllocationRequest at 8-byte alignment.
    request_data.set_align(8);
    let element_address = module.declare_data_in_data(element, &mut request_data);
    request_data.write_data_addr(0, element_address, 0);
    let descriptor_address = module.declare_data_in_data(request, &mut request_data);
    request_data.write_data_addr(16, descriptor_address, 32);
    module.define_data(request, &request_data)?;
    Ok((pointer_map, element, request))
}

impl CodegenInput<'_> {
    /// Build the immutable element descriptor + allocation request symbols for one keyed array
    /// plan. Shared by literal-keyed and bulk-call-keyed plans so the static-data emission path
    /// (`emit_array_static_data`) and the module-data collection stay single-implementation.
    ///
    /// `key` is the syntax node the plan is keyed on (an `ArrayLiteralExpression` for literals, a
    /// `CallExpression` for bulk calls); it identifies the plan's source unit and node for symbol
    /// minting. `element_type` is the declared element ABI; `length` is the element count.
    fn build_array_static_plan(
        &self,
        key: AstNodeKey,
        element_type: SemanticTypeId,
        length: u64,
    ) -> Option<ArrayStaticPlan> {
        let (stride, alignment, pointer) = scalar_layout(self.target().pointer_width, element_type)?;
        let descriptor =
            self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidArrayElementDescriptor")?;
        let request =
            self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidArrayAllocationRequest")?;
        if descriptor.size != 32 || descriptor.alignment != 8 || request.size != 32 || request.alignment != 8 {
            return None;
        }
        let unit = self
            .typed_program()
            .assembly
            .units
            .iter()
            .position(|unit| paths_match(&unit.path, key.unit.path(self.database())))?;
        let namespace = self
            .artifact_namespace()
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
            .collect::<String>();
        let identity = format!("{namespace}_u{unit}_g{}_n{}", key.generation.0, key.node.0);
        Some(ArrayStaticPlan {
            literal: key,
            pointer_map_symbol: format!("__beskid_array_pointer_map_{identity}"),
            element_descriptor_symbol: format!("__beskid_array_element_descriptor_{identity}"),
            allocation_request_symbol: format!("__beskid_array_allocation_request_{identity}"),
            element_type,
            stride,
            alignment,
            pointer_map_offsets: pointer.then_some(0_u64).into_iter().collect::<Vec<_>>().into(),
            length,
        })
    }

    /// Create source-authorized typed-array metadata.
    ///
    /// A non-empty literal proves its element ABI from every element. An empty literal is valid
    /// only when the semantic contract proves it is the direct value of a declared nominal
    /// aggregate `T[]` field. Guessing from a machine-word default would reintroduce untraced
    /// pointers, so all other empty literals remain unavailable.
    pub fn array_static_plan(&self, literal: AstNodeKey) -> Option<ArrayStaticPlan> {
        (node_kind(self.database(), literal).ok().flatten() == Some(IndexedNodeKind::ArrayLiteralExpression))
            .then_some(())?;
        let elements = child_nodes(self.database(), literal).ok().flatten()?;
        let element_type = match elements.first().copied() {
            Some(first) => {
                let element_type = node_type(self.database(), first).ok().flatten()?;
                elements
                    .iter()
                    .copied()
                    .all(|element| node_type(self.database(), element).ok().flatten() == Some(element_type))
                    .then_some(element_type)?
            }
            None => empty_array_literal_element_abi_type(self.database(), literal).ok().flatten()?,
        };
        let length = u64::try_from(elements.len()).ok()?;
        self.build_array_static_plan(literal, element_type, length)
    }

    /// Create source-authorized typed-array metadata for one `bulk`-parameter call.
    ///
    /// A bulk callee declares a `bulk T[]` parameter; the call site packs N scalar arguments into
    /// a fresh rooted array before the direct call. There is no `ArrayLiteralExpression` node to
    /// key a plan on, so this plan is keyed on the `CallExpression` node. The element ABI comes
    /// from the callee's declared `bulk T[]` parameter (declared type over inferred — the same
    /// authority `array_static_plan` uses for empty literals), and the length comes from the
    /// call's scalar argument count. Reuses `build_array_static_plan` so the static-data emission
    /// path is shared with literal plans.
    pub fn bulk_array_static_plan(&self, call: AstNodeKey) -> Option<ArrayStaticPlan> {
        (node_kind(self.database(), call).ok().flatten() == Some(IndexedNodeKind::CallExpression)).then_some(())?;
        let CallLowering::Direct(declaration) = call_lowering(self.database(), call).ok().flatten()? else {
            return None;
        };
        let parameters = child_nodes(self.database(), declaration).ok().flatten()?;
        let mut element_type = None;
        for parameter in parameters.iter().copied() {
            if node_kind(self.database(), parameter).ok().flatten() != Some(IndexedNodeKind::Parameter) {
                continue;
            }
            if let Some(fact) = bulk_parameter(self.database(), parameter).ok().flatten() {
                element_type = Some(fact.element_abi_type);
                break;
            }
        }
        let element_type = element_type?;
        let arguments = call_arguments(self.database(), call).ok().flatten()?;
        let length = u64::try_from(arguments.len()).ok()?;
        self.build_array_static_plan(call, element_type, length)
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

fn write_word(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), ModuleError> {
    let destination = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| ModuleError::Backend(anyhow::anyhow!("array static-data layout overflow")))?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
