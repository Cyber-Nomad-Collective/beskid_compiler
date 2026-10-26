//! Collection operations, managed references, cleanup plans, and local slots.

use std::sync::Arc;

use cranelift_codegen::ir::{Signature, Type};

use super::*;
use crate::layout::{ArrayLayout, ManagedStructAllocation};

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
    AggregateField { root: LocalSlotId, receiver: AstNodeKey, field_index: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionOperation {
    Append { owner: CollectionMutationOwner },
    TryAppend { owner: CollectionMutationOwner },
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

/// The array a `for` statement iterates: the element layout the loop reads with (stride and CLIF
/// element type; the length is read from the array header at loop entry) and the GC classification
/// of the elements, which decides whether the loop variable is a rooted managed local.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayForFact {
    pub layout: ArrayLayout,
    pub element_managed: ManagedReferenceFact,
}

/// The nominal iterator a `for` statement iterates through the iterator protocol: the exact
/// `Next()` callee (receiver pointer to `Option<T>` pointer), the physical `Option<T>` layout and
/// the index of its `Some` variant (whose single payload field is the element the loop variable
/// reads), and the GC classification of the elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IteratorForFact {
    pub next: DirectCallee,
    pub next_signature: Signature,
    pub option_layout: crate::EnumLayout,
    pub some_variant: u32,
    pub element_managed: ManagedReferenceFact,
}

/// The `Copy` hook of a `[CopyOnAssign]` type that a new name of a value calls (v0.6.7 copy on
/// assignment, copy-on-write). The hook takes the value as its receiver and returns the copy
/// that the new name holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueCopyPlan {
    pub callee: DirectCallee,
    pub signature: Signature,
}

impl ValueCopyPlan {
    /// The hook declaration, the site a lowering error names when no source node does (a capture).
    pub fn site(&self) -> Option<AstNodeKey> {
        match &self.callee {
            DirectCallee::Item(key) | DirectCallee::SpecializedItem { declaration: key, .. } => Some(*key),
            _ => None,
        }
    }
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
    /// Global `init` then `startup` hooks, run once the container is active.
    pub startup: Vec<CompositionHookCall>,
    /// Global `dispose` hooks, run before the container shuts down.
    pub shutdown: Vec<CompositionHookCall>,
}

/// One call of a generated host-hook function. Its arguments are the host or scope record, when
/// the hook takes one, then the hook's argument object (its injected services), which the
/// launch installed in `arguments_slot`. A hook takes at most these two arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionHookCall {
    pub function: DirectCallee,
    pub signature: Signature,
    /// The generated `let` of the `launch`/`with` site that holds the record passed first.
    pub record: Option<AstNodeKey>,
    pub arguments_slot: Option<u32>,
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
    /// For a contract-typed field: the box each target slot's implementation is stored in, one
    /// per `target_slots` entry (runtime contract dispatch, v0.6.7).
    pub boxes: Vec<Option<ContractBoxPlan>>,
}

/// What the receiver word of a contract box holds (runtime contract dispatch, v0.6.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractBoxReceiverFact {
    /// A managed reference: the box's pointer map covers the word.
    Reference,
    /// A primitive of this Cranelift type, widened to one word.
    Scalar(Type),
}

/// Boxing of a concrete value into a contract-typed destination: allocate a contract box with
/// `allocation_request_symbol`, store the receiver word at 16 and the address of the witness table
/// `witness_symbol` at 24.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractBoxPlan {
    pub allocation_request_symbol: std::sync::Arc<str>,
    pub witness_symbol: std::sync::Arc<str>,
    pub receiver: ContractBoxReceiverFact,
}

/// Conversion of a contract value to a contract its own contract embeds: the receiver word is
/// copied into a new box whose witness table is word `table_word` of the source box's table;
/// the new box takes the reference or scalar allocation request by the table's receiver kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractUpcastPlan {
    pub table_word: u32,
    pub reference_request_symbol: std::sync::Arc<str>,
    pub scalar_request_symbol: std::sync::Arc<str>,
}

/// A contract method call through a contract box: the receiver word and witness table are loaded
/// from the box, the entry at `slot` is called indirectly with `signature` (receiver word first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractDispatchCall {
    pub slot: u32,
    pub signature: Signature,
}

/// One source-keyed scope bracket with an exact frozen scope ID and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionScopePlan {
    pub site: AstNodeKey,
    pub scope_id: u32,
    pub parent_scope_id: u32,
    pub body: AstNodeKey,
    /// The registrations this scope creates anew on each activation (its non-`single` ones), in
    /// activation order; their slots refer to the container's slot numbering.
    pub registrations: Vec<CompositionRegistrationPlan>,
    /// The scope's `init` hooks, run after the scope is entered and before the body.
    pub init: Vec<CompositionHookCall>,
    /// The scope's `dispose` hooks, run on every exit from the body before the scope is left.
    pub dispose: Vec<CompositionHookCall>,
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
