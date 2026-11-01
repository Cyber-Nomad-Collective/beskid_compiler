//! Integration tests for `beskid_analysis`: one module per topic. `tests/fixtures/` holds the
//! `parse_tree_stability` golden table.

mod artifact_roundtrip;
mod attribute_sites;
mod builtin_signedness;
mod canonical_runtime_source;
mod dependency_change;
mod dependency_edit;
mod dependency_graph_intent;
mod dependency_registry;
mod dependency_transaction;
mod documentation_indentation;
mod expanded_syntax_index;
mod format_attribute_sites;
mod generic_impl_enum;
mod keyword_boundaries;
mod match_guards;
mod module_constants;
mod module_namespace;
mod move_syntax;
mod native_mod_descriptor;
mod native_mod_dispatch;
mod package_identity;
mod parse_tree_stability;
mod process_sources;
mod scoped_use;
mod serialization_mod_sources;
mod struct_literal_conditions;
mod structural_contribution_arena;
#[cfg(any(unix, windows))]
mod unit_builder_origin;
