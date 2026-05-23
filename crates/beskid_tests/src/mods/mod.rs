//! End-to-end compiler-mod pipeline tests.
//!
//! Drives the `mod.load` → `mod.collect` → `mod.generate` → `mod.analyze` →
//! `mod.rewrite` chain against the reference mod fixture under
//! `crates/beskid_tests/fixtures/mods/sample_mod/`. The tests use
//! [`beskid_analysis::mod_host::ScriptedContractInvoker`] (or the default stub)
//! to assert dispatch order, registration counts, and diagnostic codes per the
//! platform-spec compiler-mods hub.

mod conflicts;
mod contract_dispatch;
mod fixture;
