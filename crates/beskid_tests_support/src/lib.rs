//! Shared filesystem helpers for project-resolution and workspace test suites.
//!
//! Used by `beskid_tests_projects` and `beskid_tests_mods`. This crate has no external
//! dependencies (std only) so it adds nothing to the dep graph of its consumers.

pub mod test_harness;
