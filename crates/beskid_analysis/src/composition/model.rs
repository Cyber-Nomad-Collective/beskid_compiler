use std::collections::HashMap;

use crate::syntax::{InjectQualifier, SpanInfo};

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
    pub target_slots: Vec<ServiceSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindingPlan {
    /// Initialization-safe activation order with every registration assigned one immutable slot.
    pub activation: Vec<ActivationPlanEntry>,
    /// Compiler-materialized plural injection targets. Runtime lookup is never required.
    pub plurals: Vec<PluralPlan>,
    pub scope_parents: HashMap<ScopeId, Option<ScopeId>>,
}

impl BindingPlan {
    pub fn slot_for_registration(&self, registration_id: u32) -> Option<ServiceSlot> {
        self.activation.iter().find(|entry| entry.registration_id == registration_id).map(|entry| entry.slot)
    }
}
