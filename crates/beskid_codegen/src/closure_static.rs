//! Static, artifact-owned closure allocation metadata and generation-safe root authority.
//!
//! Static planning turns the current syntax generation's closure capture facts into deterministic
//! descriptor/pointer-map/allocation-request identities. Rooting never invents a TLS pointer:
//! generated code may only call the manifest-owned current-thread helper.

use std::sync::Arc;

use beskid_queries::{
    AstNodeKey, CaptureStorageClass, ClosureCapture, SemanticTypeId, closure_signature, node_kind,
};
use cranelift_module::{DataDescription, DataId, Linkage, Module, ModuleError, ModuleResult};

use crate::CodegenInput;

/// Manifest-approved ABI-v5 helpers consumed by captured-closure lowering.
pub const ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE: &str = "beskid_rt_v5_closure_environment_allocate";
pub const ABI_V5_CLOSURE_CAPTURE_STORE: &str = "beskid_rt_v5_closure_capture_store";
pub const ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT: &str =
    "beskid_rt_v5_closure_environment_root_current";

/// The source capture represented by one static closure-environment field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureCaptureStaticField {
    pub capture: ClosureCapture,
    pub abi_type: SemanticTypeId,
    pub field_offset: u64,
    pub pointer_map_index: Option<u64>,
}

/// Opaque runtime-root authority is intentionally absent from static descriptor planning.
///
/// CYB-127 owns the only permitted route for carrying an actual current TLS/root-frame context
/// into generated code.  An uninhabited type makes it impossible for this plan to manufacture
/// one from source identities, a stack slot, or a null value.
#[derive(Debug)]
pub enum RuntimeRootContext {}

/// Source-authorized current-thread root ownership for one closure lowering site.
///
/// This fact never carries a TLS or root-frame pointer. Lowering may only emit a call to
/// [`ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT`] with the reserved `slot_index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureRootAuthority {
    pub slot_index: u64,
    pub root_helper: &'static str,
}

/// Generation-safe authority required before captured-closure ISLE lowering may proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureLoweringAuthority {
    pub plan: ClosureStaticPlan,
    pub root: ClosureRootAuthority,
}

/// Deterministic static data required by ABI-v5 closure-environment allocation.
///
/// The three symbols identify artifact-owned storage; this fact does not claim that storage has
/// been emitted yet, and it cannot issue the runtime root call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureStaticPlan {
    pub lambda: AstNodeKey,
    pub descriptor_symbol: String,
    pub pointer_map_symbol: String,
    pub allocation_request_symbol: String,
    pub object_size: u64,
    pub object_alignment: u64,
    pub pointer_map_offsets: Arc<[u64]>,
    pub captures: Arc<[ClosureCaptureStaticField]>,
}

/// Data definitions materialized from one [`ClosureStaticPlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureStaticDataHandles {
    pub descriptor: DataId,
    pub pointer_map: DataId,
    pub allocation_request: DataId,
}

impl ClosureStaticPlan {
    /// Static planning never has a runtime value for the current TLS/root frame.
    pub const fn runtime_root_context(&self) -> Option<RuntimeRootContext> {
        None
    }
}

impl ClosureRootAuthority {
    /// Construct current-thread root authority only when the helper is the canonical export.
    pub fn current_thread(slot_index: u64) -> Option<Self> {
        Some(Self {
            slot_index,
            root_helper: ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT,
        })
    }
}

/// Define only the artifact-owned static data for one closure environment.
///
/// This emits no calls and imports no runtime symbol. In particular, it cannot root an object:
/// the opaque runtime TLS/root-frame input is intentionally outside this API.
pub fn emit_closure_static_data<M: Module>(
    module: &mut M,
    plan: &ClosureStaticPlan,
) -> ModuleResult<ClosureStaticDataHandles> {
    let pointer_map =
        module.declare_data(&plan.pointer_map_symbol, Linkage::Local, false, false)?;
    let descriptor = module.declare_data(&plan.descriptor_symbol, Linkage::Local, false, false)?;
    let allocation_request = module.declare_data(
        &plan.allocation_request_symbol,
        Linkage::Local,
        false,
        false,
    )?;

    let mut pointer_map_bytes = Vec::with_capacity(plan.pointer_map_offsets.len().max(1) * 8);
    if plan.pointer_map_offsets.is_empty() {
        pointer_map_bytes.extend_from_slice(&0u64.to_le_bytes());
    } else {
        for offset in plan.pointer_map_offsets.iter().copied() {
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
        u64::try_from(plan.pointer_map_offsets.len()).map_err(|_| {
            ModuleError::Backend(anyhow::anyhow!(
                "closure pointer-map length exceeds ABI word"
            ))
        })?,
    )?;
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
    module.define_data(allocation_request, &request_data)?;

    Ok(ClosureStaticDataHandles {
        descriptor,
        pointer_map,
        allocation_request,
    })
}

fn write_word(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), ModuleError> {
    let destination = bytes.get_mut(offset..offset + 8).ok_or_else(|| {
        ModuleError::Backend(anyhow::anyhow!("closure static-data layout overflow"))
    })?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

impl CodegenInput<'_> {
    /// Combine a current-generation static plan with a reserved root-slot owner for one site.
    ///
    /// Absent plans, missing manifest helpers, or stack-reference captures remain fail-closed.
    /// Ordinary syntax cannot name TLS or manufacture a root-frame pointer through this API.
    pub fn closure_lowering_authority(
        &self,
        site: AstNodeKey,
        lambda: AstNodeKey,
    ) -> Option<ClosureLoweringAuthority> {
        let plan = self.closure_static_plan(lambda)?;
        if !self.manifest_exports_symbol(ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE)
            || !self.manifest_exports_symbol(ABI_V5_CLOSURE_CAPTURE_STORE)
            || !self.manifest_exports_symbol(ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT)
        {
            return None;
        }
        let slot_index = root_slot_index(self, site)?;
        let root = ClosureRootAuthority::current_thread(slot_index)?;
        Some(ClosureLoweringAuthority { plan, root })
    }

    fn manifest_exports_symbol(&self, symbol: &str) -> bool {
        self.abi_manifest()
            .exports
            .iter()
            .any(|export| export.symbol == symbol)
    }

    /// Produce static closure metadata only for a current, transferable-capture lambda.
    ///
    /// Missing/stale/foreign keys, non-lambdas, unsupported ABI shapes, and stack-reference
    /// captures return `None`.  Callers must not substitute descriptors, allocation requests, or
    /// root values when this proof is absent.
    pub fn closure_static_plan(&self, lambda: AstNodeKey) -> Option<ClosureStaticPlan> {
        if !matches!(
            node_kind(self.database(), lambda),
            Ok(Some(beskid_queries::IndexedNodeKind::LambdaExpression))
        ) {
            return None;
        }
        let signature = match closure_signature(self.database(), lambda) {
            Ok(Some(signature)) => signature,
            _ => {
                return None;
            }
        };
        if signature.lambda != lambda
            || signature
                .environment
                .fields
                .iter()
                .any(|field| field.capture.class != CaptureStorageClass::TransferableValue)
        {
            return None;
        }

        let header = self
            .abi_manifest()
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidObjectHeader");
        let descriptor = self
            .abi_manifest()
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidTypeDescriptor");
        let request = self
            .abi_manifest()
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidAllocationRequest");
        let (Some(header), Some(descriptor), Some(request)) = (header, descriptor, request) else {
            return None;
        };
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
        let mut captures = Vec::with_capacity(signature.environment.fields.len());
        for field in signature.environment.fields.iter() {
            let Some((field_size, field_alignment, is_pointer)) =
                scalar_layout(self.target().pointer_width, field.abi_type)
            else {
                return None;
            };
            size = align_to(size, field_alignment)?;
            let field_offset = size;
            size = size.checked_add(field_size)?;
            alignment = alignment.max(field_alignment);
            let pointer_map_index = is_pointer
                .then(|| u64::try_from(pointer_map_offsets.len()).ok())
                .flatten();
            if is_pointer {
                pointer_map_offsets.push(field_offset);
            }
            captures.push(ClosureCaptureStaticField {
                capture: field.capture,
                abi_type: field.abi_type,
                field_offset,
                pointer_map_index,
            });
        }
        if !valid_alignment(alignment) {
            return None;
        }
        let object_size = align_to(size, alignment)?;
        let Some(identity) = closure_identity(self, lambda) else {
            return None;
        };
        Some(ClosureStaticPlan {
            lambda,
            descriptor_symbol: format!("__beskid_closure_descriptor_{identity}"),
            pointer_map_symbol: format!("__beskid_closure_pointer_map_{identity}"),
            allocation_request_symbol: format!("__beskid_closure_allocation_request_{identity}"),
            object_size,
            object_alignment: alignment,
            pointer_map_offsets: pointer_map_offsets.into(),
            captures: captures.into(),
        })
    }
}

fn closure_identity(input: &CodegenInput<'_>, lambda: AstNodeKey) -> Option<String> {
    let key_path = lambda.unit.path(input.database());
    let unit_index = input
        .typed_program()
        .assembly
        .units()
        .iter()
        .position(|unit| paths_match(&unit.path, key_path))?;
    Some(format!(
        "u{unit_index}_g{}_n{}",
        lambda.generation.0, lambda.node.0
    ))
}

/// Reserve one deterministic root-slot owner identity for a lowering site.
///
/// The index is derived from the site's generation-safe syntax identity so two call/spawn sites
/// never share a slot reservation without also sharing that exact syntax key.
fn root_slot_index(input: &CodegenInput<'_>, site: AstNodeKey) -> Option<u64> {
    let identity = closure_identity(input, site)?;
    let mut hash = 0u64;
    for byte in identity.as_bytes() {
        hash = hash.wrapping_mul(131).wrapping_add(u64::from(*byte));
    }
    Some(hash % 64)
}

fn paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}

fn scalar_layout(pointer_width: u8, ty: SemanticTypeId) -> Option<(u64, u64, bool)> {
    let pointer_bytes = u64::from(pointer_width.checked_div(8)?);
    if !valid_alignment(pointer_bytes) {
        return None;
    }
    match ty {
        SemanticTypeId::UNIT | SemanticTypeId::NEVER => Some((0, 1, false)),
        SemanticTypeId::BOOL | SemanticTypeId::U8 => Some((1, 1, false)),
        SemanticTypeId::I32 | SemanticTypeId::CHAR => Some((4, 4, false)),
        SemanticTypeId::I64 | SemanticTypeId::F64 => Some((8, 8, false)),
        SemanticTypeId::WORD => Some((pointer_bytes, pointer_bytes, false)),
        SemanticTypeId::POINTER | SemanticTypeId::STRING => {
            Some((pointer_bytes, pointer_bytes, true))
        }
        _ => None,
    }
}

fn align_to(value: u64, alignment: u64) -> Option<u64> {
    if !valid_alignment(alignment) {
        return None;
    }
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

const fn valid_alignment(alignment: u64) -> bool {
    alignment != 0 && alignment.is_power_of_two()
}
