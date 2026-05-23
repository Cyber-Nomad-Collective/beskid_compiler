//! Native dependency-injection integration tests.
//!
//! Covers the v0.3 deliverables of the `language-meta/composition/dependency-injection`
//! feature: scope enter/leave, plural inject, and reverse-order dispose through the
//! runtime container plus codegen lowering.

#[cfg(test)]
mod container;

#[cfg(test)]
mod host_e2e;

#[cfg(test)]
mod lowering;
