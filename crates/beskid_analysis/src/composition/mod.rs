//! Native IoC composition analysis: host-chain merge, scope tree, dependency graph, and snapshot.

pub mod baseline;
pub mod collect;
pub mod container;
pub mod diagnostics;
pub mod graph;
pub mod host_chain;
pub mod model;
pub mod pass;
pub mod resolve_inject;
pub mod scope_tree;
pub mod snapshot;

pub use diagnostics::{CompositionIssue, composition_issue_code};
pub use model::{
    BindingPlan, CompositionHost, CompositionScope, Registration, RegistrationKey,
    RegistrationLifetime, ScopeId,
};
pub use pass::{CompositionInput, CompositionResult, resolve_composition};
pub use snapshot::CompositionSnapshot;
