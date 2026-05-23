//! Integration tests for the `beskid` CLI subcommands.
//!
//! Workspace-aware tests live here (not in `beskid_cli/tests/`) so they can re-use the shared
//! analysis / project / runtime stacks already in scope. Cover end-to-end CLI flows that touch
//! filesystem state, manifest mutation, and registry behavior.

pub mod import_lib;
