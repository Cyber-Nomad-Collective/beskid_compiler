//! Canonical aggregate, enum, and field-layout fact implementations.

mod aggregate;
mod common;
mod enum_layout;
mod field_access;

pub(in crate::semantic_contract) use aggregate::{
    aggregate_layout_tracked, aggregate_literal_declaration_tracked, array_index_element_abi_type_tracked,
    empty_array_literal_element_abi_type_tracked,
};

pub(in crate::semantic_contract) use common::{
    abi_local_declaration_type, abi_type_for_direct_aggregate_field_projection, abi_type_for_local_path,
    aggregate_field_layout, nominal_aggregate_abi_type, resolve_nominal_layout_declaration, resolve_type_declaration,
    semantic_type_from_syntax, unique_exported_type_in_unit, unique_public_type_in_unit, unique_type_in_unit,
};

pub(in crate::semantic_contract) use enum_layout::{
    aggregate_shape_from_applied_type, contextual_enum_constructor_type_path, enum_constructor_tracked,
    enum_field_layout, enum_layout_from_definition, enum_layout_substitutions, enum_layout_tracked,
    enum_match_scrutinee_layout, enum_match_tracked, enum_pattern_targets_declaration,
    instantiated_enum_layout_for_path,
};

pub(in crate::semantic_contract) use field_access::{
    aggregate_field_access_tracked, nominal_local_receiver_declaration,
};
