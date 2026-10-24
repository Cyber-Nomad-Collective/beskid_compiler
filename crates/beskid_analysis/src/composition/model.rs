use std::collections::HashMap;

use crate::syntax::{AstNodeId, InjectQualifier, ScopeHookKind, SpanInfo};

/// The source unit a composition declaration lives in: `None` is the unit under analysis (the
/// entry), `Some(path)` an imported unit of the same assembly, such as a library's base host.
pub type UnitPath = Option<std::sync::Arc<std::path::Path>>;

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

impl RegistrationKey {
    /// The source name of the contract or self-bound type, as written in the registry.
    pub fn name(&self) -> &str {
        match self {
            Self::Contract(name) | Self::SelfType(name) => name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationLifetime {
    Scoped,
    Single,
    Transient,
}

impl RegistrationLifetime {
    /// The lifetime as source spells it; a scoped registration has no keyword.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Scoped => "scoped",
            Self::Single => "single",
            Self::Transient => "transient",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub id: u32,
    /// Exact registry-entry syntax identity; the backend never rediscovers it by name.
    pub source_node_id: AstNodeId,
    /// The unit that declares the registry line.
    pub unit: UnitPath,
    pub scope_id: ScopeId,
    pub key: RegistrationKey,
    pub implementation: String,
    pub lifetime: RegistrationLifetime,
    pub span: SpanInfo,
    /// The implementation path of the registry line (`Impl` in `single Impl for Contract;`).
    pub implementation_span: SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectDependency {
    pub span: SpanInfo,
    pub field_node_id: AstNodeId,
    pub field_unit: UnitPath,
    pub owner_registration_id: u32,
    pub requested_type: String,
    pub is_plural: bool,
    pub qualifier: Option<InjectQualifier>,
    pub field_name: String,
    pub name_span: SpanInfo,
    pub type_span: SpanInfo,
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
    pub unit: UnitPath,
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
    pub field_node_id: AstNodeId,
    pub field_unit: UnitPath,
    pub target_slots: Vec<ServiceSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingularPlan {
    pub owner_registration_id: u32,
    pub field_span: SpanInfo,
    pub field_node_id: AstNodeId,
    pub field_unit: UnitPath,
    pub target_slot: ServiceSlot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionHookPlan {
    pub scope_id: ScopeId,
    pub kind: ScopeHookKind,
    pub source_node_id: AstNodeId,
    pub span: SpanInfo,
    /// The unit of the host that declares the hook (its generated items live there too).
    pub unit: UnitPath,
    /// The generated function that holds the hook body (see [`super::hooks`]).
    pub function_node_id: AstNodeId,
    /// The registration of the generated argument type whose injected fields are the hook
    /// parameters in order; `None` for a hook without parameters.
    pub arguments_registration_id: Option<u32>,
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
    /// `init` hooks in host-chain order (base host first), each run when its scope is entered;
    /// a global `init` runs once at launch, before `startup`.
    pub init_hooks: Vec<CompositionHookPlan>,
    /// Global `startup` hooks in host-chain order, run once after the container is active.
    pub startup_hooks: Vec<CompositionHookPlan>,
    /// `dispose` hooks in reverse host-chain order (launched host first), each run when its
    /// scope is left; a global `dispose` runs once before the container shuts down.
    pub disposal_hooks: Vec<CompositionHookPlan>,
}

impl BindingPlan {
    pub fn slot_for_registration(&self, registration_id: u32) -> Option<ServiceSlot> {
        self.activation.iter().find(|entry| entry.registration_id == registration_id).map(|entry| entry.slot)
    }
}
