//! Shared filesystem and native-fixture helpers for test suites.
//!
//! Without features this crate has no dependencies (std only), so it adds nothing to the
//! dependency graph of its consumers. The `source-facts` feature adds the Beskid parser.

pub mod evidence;
pub mod native_harness;
#[cfg(feature = "source-facts")]
pub mod source_facts;
pub mod test_harness;

pub use test_harness::*;
