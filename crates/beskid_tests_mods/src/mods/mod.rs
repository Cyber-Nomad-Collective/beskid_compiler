//! Compiler-mod fixture tests that need no native Mod execution.
//!
//! Scheduling, dispatch order and registration-conflict tests live beside the scheduler in
//! `beskid_analysis` (`mod_host::api::scheduling_tests`) because a qualified executable Mod
//! descriptor cannot be forged by fixtures; real executable qualification is covered by the CLI
//! native Mod tests. This crate keeps lockfile replay, generated-output layout and the rule that
//! a legacy object descriptor never qualifies dispatch.

mod fixture;
mod generate_output;
mod rebuild;
