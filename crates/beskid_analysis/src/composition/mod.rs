//! Native IoC composition analysis: host-chain merge, scope tree, dependency graph, and snapshot.

pub mod baseline;
pub mod collect;
pub mod container;
pub mod diagnostics;
pub mod graph;
pub mod hooks;
pub mod host_chain;
pub mod model;
pub mod pass;
pub mod resolve_inject;
pub mod scope_tree;
pub mod shapes;
pub mod snapshot;

pub use diagnostics::{CompositionIssue, UnconstructibleReason, composition_issue_code};
pub use hooks::{is_hook_item_name, lower_host_hooks};
pub use model::{
    ActivationPlanEntry, BindingPlan, CompositionHookPlan, CompositionHost, CompositionScope, PluralPlan, Registration,
    RegistrationKey, RegistrationLifetime, ScopeId, ServiceSlot, SingularPlan, UnitPath,
};
pub use pass::{CompositionInput, CompositionResult, resolve_composition, resolve_composition_with_imports};
pub use snapshot::CompositionSnapshot;
