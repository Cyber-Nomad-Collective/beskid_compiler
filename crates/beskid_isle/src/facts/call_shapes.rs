//! Direct callees, spawn entries, closure environments, and lambda entries.

use super::*;
use cranelift_codegen::ir::Type;

/// Exact semantic call target.
///
/// Source items carry their complete generation-safe syntax key.  A node id is only unique
/// within one source unit and revision, so using it as a module-import key can bind a call to an
/// unrelated item when two units happen to assign the same local id. Runtime intrinsics are not
/// source items and retain their canonical ABI-table index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DirectCallee {
    Item(AstNodeKey),
    /// One generic source declaration paired with its exact call-derived ABI identity.
    ///
    /// The vector stores parameter ABI type identities followed by the result identity.  It is
    /// deliberately structural rather than a lossy hash so module imports cannot conflate two
    /// valid generic instantiations.
    SpecializedItem {
        declaration: AstNodeKey,
        abi_identity: std::sync::Arc<[u32]>,
    },
    RuntimeIntrinsic(u32),
    /// One compiler-authorized Corelib syscall ABI service, identified by its manifest symbol.
    ///
    /// This is intentionally distinct from [`Self::RuntimeIntrinsic`]: Corelib source authority
    /// is not canonical-runtime intrinsic authority and cannot reuse its capability token.
    CorelibService(&'static str),
    /// One generated ABI-v5 fiber entry trampoline, keyed by its source `spawn` expression.
    SpawnTrampoline(AstNodeKey),
    /// One generated ABI-v5 closure entry trampoline, keyed by its source `LambdaExpression`.
    LambdaTrampoline(AstNodeKey),
}

impl DirectCallee {
    pub const fn item(key: AstNodeKey) -> Self {
        Self::Item(key)
    }

    pub fn specialized_item(declaration: AstNodeKey, abi_identity: impl Into<std::sync::Arc<[u32]>>) -> Self {
        Self::SpecializedItem { declaration, abi_identity: abi_identity.into() }
    }

    pub const fn runtime_intrinsic(index: u32) -> Self {
        Self::RuntimeIntrinsic(index)
    }

    pub const fn corelib_service(symbol: &'static str) -> Self {
        Self::CorelibService(symbol)
    }

    pub const fn spawn_trampoline(spawn: AstNodeKey) -> Self {
        Self::SpawnTrampoline(spawn)
    }

    pub const fn lambda_trampoline(lambda: AstNodeKey) -> Self {
        Self::LambdaTrampoline(lambda)
    }
}

/// Exact source entry selected for the first executable spawn lowering leaf.
///
/// Capture-free, argument-free entries keep a null environment. Capture-proven lambda entries
/// carry artifact-owned allocate/store/root authority; direct item entries with eager
/// arguments carry an argument environment. The two environments are mutually exclusive;
/// unsupported capture or argument shapes remain unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnEntry {
    pub trampoline: DirectCallee,
    pub closure_environment: Option<InlineClosureEnvironment>,
    pub argument_environment: Option<SpawnArgumentEnvironment>,
    pub handle_request_symbol: std::sync::Arc<str>,
    pub handle_field_offset: i32,
}

/// One eager `spawn Entry(args)` argument stored into the fiber's managed start environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnArgumentField {
    pub argument: AstNodeKey,
    pub field_offset: i32,
    pub value_type: Type,
}

/// Artifact-owned managed start environment for a direct item spawn with arguments.
///
/// The parent evaluates every argument in source order, allocates this descriptor-backed object
/// through `beskid_rt_v5_managed_object_allocate`, stores the values, and passes the object as
/// the fiber environment. The runtime roots it for the fiber's lifetime; the generated spawn
/// trampoline loads the fields and calls the entry with them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnArgumentEnvironment {
    pub allocation_request_symbol: std::sync::Arc<str>,
    pub fields: Vec<SpawnArgumentField>,
}

/// Manifest-derived destination slot and boxed-value offsets for typed Fiber Join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TracedFiberJoinLayout {
    pub symbol: &'static str,
    pub slot_size: u32,
    pub alignment_shift: u8,
    pub payload_offset: i32,
    pub value_offset: i32,
}

/// One transferable capture field stored into an ABI-v5 closure environment before a call/spawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineCaptureField {
    pub local_slot: LocalSlotId,
    pub field_offset: u32,
    pub pointer_map_index: Option<u64>,
    pub value_type: Type,
}

/// Artifact-owned allocate/store/root facts for a capturing immediate call or spawn.
///
/// The symbols name module-local static data. Rooting always uses the current-thread helper; no
/// TLS pointer is ever supplied through this fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineClosureEnvironment {
    pub allocation_request_symbol: std::sync::Arc<str>,
    pub descriptor_symbol: std::sync::Arc<str>,
    pub captures: Vec<InlineCaptureField>,
}

/// An immediate lambda call selected from current syntax facts.
///
/// Capture-free calls remain allocation-free. Capturing calls carry ABI-v5 environment authority
/// and otherwise remain unavailable; there is no dynamic closure fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineLambdaCall {
    pub body: AstNodeKey,
    pub parameters: Vec<ParameterSlot>,
    pub result_type: Type,
    pub closure_environment: Option<InlineClosureEnvironment>,
}

/// Exact source entry selected for one freestanding lambda expression lowering leaf.
///
/// Capture-free entries keep a null environment. Capture-proven entries carry artifact-owned
/// allocate/store/root authority; unsupported capture shapes remain unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaEntry {
    pub trampoline: DirectCallee,
    pub closure_environment: Option<InlineClosureEnvironment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallImportError {
    UnknownCallee,
}
