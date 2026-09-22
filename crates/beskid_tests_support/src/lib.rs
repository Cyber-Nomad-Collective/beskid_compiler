//! Shared filesystem and native-fixture helpers for test suites.
//!
//! This crate has no external dependencies (std only), so it adds nothing to the
//! dependency graph of its consumers.

pub mod native_harness;
pub mod test_harness;

pub use test_harness::*;
