//! Native dependency-injection runtime container.
//!
//! Implements the runtime contract behind the language-meta
//! `host` / `registry` / `scope` / `with` / `launch` surface. Mirrors the
//! [`beskid_analysis::composition::BindingPlan`] (registration ordering, scope tree,
//! plural bindings) so that codegen-lowered DI sites can hand off to a single typed surface.
//!
//! The container intentionally stays in `beskid_runtime` (no new crates) and exposes a
//! C-compatible facade through [`crate::builtins::composition`].

pub mod container;
pub mod registry;
pub mod scope;

pub use container::{
    ContainerError, DisposeHook, InitHook, InstanceFactory, InstancePtr, RegistrationRecord,
    RuntimeContainer,
};
pub use registry::{Lifetime, RegistrationId, ScopeId};
pub use scope::{ActiveScope, ScopeStack};
