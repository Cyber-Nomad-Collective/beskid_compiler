//! Collection operations, managed references, cleanup plans, and local slots.

use super::*;
use crate::layout::ManagedStructAllocation;
use cranelift_codegen::ir::{Signature, Type};

/// Scalar facts consumed by leaf ISLE rules.
///
/// The frontend adapter implements this trait with generation-checked Salsa queries. It is a
/// compile-time boundary while those queries are integrated, not a second semantic model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionMutationOwner {
    Local(LocalSlotId),
    AggregateField { receiver: LocalSlotId, field_index: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionOperation {
    Append { owner: CollectionMutationOwner },
    UnprovenMutationOwner,
    Capacity,
    Clear,
    RemoveLast,
}

/// Source-authoritative GC classification kept distinct from the physical pointer ABI.
///
/// `NativeOrScalar` includes opaque native `pointer` values. `GcManaged` is reserved for
/// strings, arrays, nominal objects/enums, and closure environments whose source type is traced
/// by the ABI-v5 collector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedReferenceFact {
    NativeOrScalar,
    GcManaged,
}

#[derive(Clone)]
pub struct ScopedCleanupPlan {
    pub binding: AstNodeKey,
    pub body: Option<AstNodeKey>,
    pub dispose: DirectCallee,
    pub dispose_signature: Signature,
    pub dispose_layout: crate::EnumLayout,
    pub conversion: Option<(DirectCallee, Signature)>,
    pub enclosing_layout: crate::EnumLayout,
    pub allocation: ManagedStructAllocation,
    pub converted_error_managed: bool,
}

/// Generation-safe local slot and scalar type for one emitted function parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalSlotId {
    pub owner_node: u32,
    pub index: u32,
}

/// Generation-safe local slot and scalar type for one emitted function parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterSlot {
    pub slot: LocalSlotId,
    pub value_type: Type,
    pub managed_reference: ManagedReferenceFact,
}
