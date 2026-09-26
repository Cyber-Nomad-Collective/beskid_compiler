//! Collection operations, managed references, cleanup plans, and local slots.

use std::sync::Arc;

use cranelift_codegen::ir::{Signature, Type};

use super::*;
use crate::layout::ManagedStructAllocation;

/// Exact query authority consumed by the event emitters. No receiver/field identity is
/// reconstructed by generated ISLE or by the backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventOperationPlan {
    pub operation: EventOperation,
    pub operation_node: AstNodeKey,
    pub receiver: AstNodeKey,
    pub receiver_slot: Option<LocalSlotId>,
    pub declaration: AstNodeKey,
    pub field: AstNodeKey,
    pub slot_offset: u32,
    pub capacity: u32,
    pub handler: Option<AstNodeKey>,
    pub handler_lambda: Option<AstNodeKey>,
    pub arguments: Arc<[AstNodeKey]>,
    pub delegate_parameters: Arc<[beskid_queries::SemanticTypeId]>,
    pub delegate_result: beskid_queries::SemanticTypeId,
}

/// Event-only materialization plan for a lambda-valued local declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventHandlerLocalPlan {
    pub lambda: AstNodeKey,
    pub trampoline: DirectCallee,
    pub closure_environment: Option<InlineClosureEnvironment>,
    pub parameters: Arc<[Type]>,
    pub result: Option<Type>,
}

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

/// One source-keyed, validated launch site with its compiler-frozen container shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionLaunchPlan {
    pub site: AstNodeKey,
    pub slot_count: u32,
    pub registrations: Vec<CompositionRegistrationPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionRegistrationPlan {
    pub slot: u32,
    pub allocation_request_symbol: std::sync::Arc<str>,
    pub injections: Vec<CompositionInjectionPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionInjectionPlan {
    pub field: AstNodeKey,
    pub field_offset: u32,
    pub target_slots: Vec<u32>,
    pub plural_allocation_request_symbol: Option<std::sync::Arc<str>>,
}

/// One source-keyed scope bracket with an exact frozen scope ID and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionScopePlan {
    pub site: AstNodeKey,
    pub scope_id: u32,
    pub parent_scope_id: u32,
    pub body: AstNodeKey,
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
