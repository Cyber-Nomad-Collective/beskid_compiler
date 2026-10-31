//! Integration tests for `beskid_queries`: one module per topic.
//!
//! `tests/incremental.rs` and `tests/mod_canonical_routes.rs` stay separate executables because
//! they change process-global state: the Salsa hit/miss counters and the configured Corelib root.

mod api_documentation;
mod bsol_computed_enum_match;
mod bsol_indexed_member_operands;
mod bsol_nested_field_operands;
mod call_argument_empty_array_context;
mod database_lifecycle;
mod enum_empty_array_context;
mod expected_type_contexts;
mod front_end_authority;
mod generation_contract;
mod glue_extension_facts;
mod mod_semantic_authority;
mod mod_semantic_functions;
mod move_crossing_corelib;
mod nested_collection_owner;
mod optional_parameters;
mod package_check_false_positives;
#[cfg(feature = "persistence")]
mod persistence;
mod prepare_diagnostics_authority;
mod primitive_profile;
mod projected_path_enum_match;
mod qualified_dependency_module_paths;
mod runtime_fixture;
mod rust_owner_facts;
mod semantic_facts;
mod serialization_template_containers;
mod transfer_class_walk;
mod try_diagnostics;
mod type_shadowing_legality;
mod unavailable_inventory;
mod undefined_argument_diagnostics;
mod unknown_module_path_diagnostics;
