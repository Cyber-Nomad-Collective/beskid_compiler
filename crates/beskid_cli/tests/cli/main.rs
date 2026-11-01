//! Integration tests for `beskid_cli`: one module per topic. `tests/fixtures/` holds the Beskid
//! sources that `clif_blocks` and `optional_extern` embed.

mod analyze_exit;
mod check_all_members;
mod cli_inventory;
mod clif_blocks;
mod corelib_package_builds;
mod default_targets;
mod definite_return;
mod dependent_channel_library;
mod glue_rust_build;
mod import_lib;
mod inline_module_scoping;
mod library_target;
mod line_interaction;
mod mut_array_parameter_growth;
mod mut_parameter_assignment;
mod native_mod_sdk;
mod native_test_execution;
mod offline_policy;
mod optional_extern;
mod output_defaults;
mod pipeline_output;
mod portable_lock_policy;
mod run_and_output;
mod template_authoring_lock;
mod test_without_test_targets;
mod toolchain_ownership;
