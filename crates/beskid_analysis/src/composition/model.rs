use std::collections::HashMap;

use crate::syntax::{AstNodeId, InjectQualifier, ScopeHookKind, SpanInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

impl ScopeId {
    pub const GLOBAL: Self = Self(0);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegistrationKey {
    Contract(String),
    SelfType(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationLifetime {
    Scoped,
    Single,
    Transient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub id: u32,
    pub scope_id: ScopeId,
    pub key: RegistrationKey,
    pub implementation: String,
    pub lifetime: RegistrationLifetime,
    pub span: SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectDependency {
    pub span: SpanInfo,
    pub owner_registration_id: u32,
    pub requested_type: String,
    pub is_plural: bool,
    pub qualifier: Option<InjectQualifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionScope {
    pub id: ScopeId,
    pub name: String,
    pub parent: Option<ScopeId>,
    pub span: SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionHost {
    pub name: String,
    pub base_host: Option<String>,
    pub span: SpanInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceSlot(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationPlanEntry {
    pub registration_id: u32,
    pub slot: ServiceSlot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluralPlan {
    pub owner_registration_id: u32,
    pub field_span: SpanInfo,
    pub target_slots: Vec<ServiceSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingularPlan {
    pub owner_registration_id: u32,
    pub field_span: SpanInfo,
    pub target_slot: ServiceSlot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionHookPlan {
    pub scope_id: ScopeId,
    pub kind: ScopeHookKind,
    pub source_node_id: AstNodeId,
    pub span: SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindingPlan {
    /// Host resolved by semantic analysis; it must agree with the accompanying snapshot.
    pub launched_host: String,
    /// Initialization-safe activation order with every registration assigned one immutable slot.
    pub activation: Vec<ActivationPlanEntry>,
    /// Compiler-resolved singular injection targets; field spans distinguish fields on one owner.
    pub singulars: Vec<SingularPlan>,
    /// Compiler-materialized plural injection targets. Runtime lookup is never required.
    pub plurals: Vec<PluralPlan>,
    pub scope_parents: HashMap<ScopeId, Option<ScopeId>>,
    pub init_hooks: Vec<CompositionHookPlan>,
    pub startup_hooks: Vec<CompositionHookPlan>,
    pub disposal_hooks: Vec<CompositionHookPlan>,
}

impl BindingPlan {
    pub fn slot_for_registration(&self, registration_id: u32) -> Option<ServiceSlot> {
        self.activation.iter().find(|entry| entry.registration_id == registration_id).map(|entry| entry.slot)
    }
}
