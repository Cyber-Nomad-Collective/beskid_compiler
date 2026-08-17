//! End-to-end compiler-mod pipeline tests.
//!
//! Drives the `mod.load` → `mod.collect` → `mod.generate` → `mod.analyze` →
//! `mod.rewrite` chain against the reference mod fixture under
//! `crates/beskid_tests_mods/fixtures/mods/sample_mod/`. The tests use
//! [`beskid_analysis::mod_host::ScriptedContractInvoker`] (or the default stub)
//! to assert dispatch order, registration counts, and diagnostic codes per the
//! platform-spec compiler-mods hub.

mod analyzer_coverage;
mod conflicts;
mod contract_dispatch;
mod fixture;
mod generate_output;
mod incremental_replay;
mod rebuild;
mod typed_merge;
