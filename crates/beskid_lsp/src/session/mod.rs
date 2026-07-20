//! Open buffers, on-disk snapshots, compilation cache, and diagnostics refresh helpers.

pub(crate) mod db_access;
pub(crate) mod diagnostics_bridge;
pub(crate) mod documentation_facts;
pub(crate) mod lifecycle;
pub(crate) mod project_context;
pub(crate) mod startup;
#[cfg(test)]
mod startup_tests;
pub(crate) mod store;
