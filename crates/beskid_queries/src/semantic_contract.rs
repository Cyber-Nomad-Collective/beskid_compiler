//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.
//!
//! This file is the stable public seam. Generation-bound implementations live in focused
//! descendants and are re-exported here without changing query names or signatures.

#![allow(unused_imports)] // Preserve the original crate-internal facade paths after extraction.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, AbiType, TargetMetadata};
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::db::Db;

mod abi;
mod bulk;
mod call_abi;
mod calls;
mod closures_spawn;
mod completion;
mod layouts;
mod locals;
mod model;
mod queries;
mod resolution;
mod syntax_facts;
mod typing;

use abi::{
    abi_signature_from_syntax, abi_type_for_binary_expression, abi_type_for_expression, abi_type_from_syntax,
    abi_type_tracked, binary_operand_abi_type_tracked, block_may_fall_through, builtin_type_to_semantic,
    call_abi_signature_for_call, call_abi_signature_tracked, call_argument_abi_type_tracked,
    contextual_constant_integer, contextual_integer_literal_abi_type_tracked, control_flow_for_node,
    control_flow_tracked, corelib_service_abi_signature, dispatch_builtin_abi_signature,
    exact_assembled_nominal_envelope, generic_abi_type, generic_parameter_reference_name,
    generic_specialization_instance_for_call, generic_type_name, if_may_fall_through, integer_has_explicit_abi_suffix,
    integer_literal_fits_abi, integer_literal_text, integer_literal_u64, item_abi_signature_tracked,
    item_abi_type_from_syntax, item_signature_for_node, item_signature_tracked, signature_from_syntax,
    statement_may_fall_through, statements_may_fall_through, type_syntax_mentions_generic_parameter,
    unsuffixed_integer_literal, value_abi_type_tracked,
};
use bulk::bulk_parameter_tracked;
use calls::{
    abi_semantic_type, call_arguments_tracked, call_lowering_for_node, call_lowering_tracked,
    canonical_intrinsic_parameter_type, canonical_result_definition_for_type, canonical_result_variant,
    canonical_runtime_intrinsic_scope, cast_intents_for_node, cast_intents_tracked, corelib_service_for,
    expected_cast_type, explicit_generic_type_argument_syntax, exported_generic_type_named, expression_fact_target,
    expression_is_lambda, flatten_member_as_path_declaration, for_iterator_fact_tracked, function_declares_generics,
    generic_call_instantiation_for_node, generic_call_instantiation_tracked, generic_call_specialization_tracked,
    generic_call_template_tracked, generic_call_uses_parameter_type_arguments, generic_callable_parameters,
    generic_nominal_method_receiver_tracked, imported_call_receiver_exists,
    imported_generic_nominal_receiver_requires_instantiation, is_transparent_binary_operand_path,
    method_declaration_for_member_receiver, nominal_local_member_receiver, nominal_member_receiver_tracked,
    primitive_integer, primitive_numeric, primitive_numeric_conversion_target, primitive_numeric_conversion_tracked,
    range_for_fact_tracked, resolve_local_extern_contract_method, result_type_parts, same_type_syntax,
    try_expression_fact_for_node, try_expression_fact_tracked, try_operand_parameter_declaration,
    type_syntax_is_generic_parameter_reference, unique_nominal_method_declaration,
};
use closures_spawn::{
    callable_signature_for_node, callable_signature_for_path, callable_signature_tracked, capture_storage_class,
    capture_storage_for_node, capture_storage_tracked, closure_call_target_tracked, closure_captures,
    closure_environment_for_node, closure_environment_tracked, closure_signature_for_node, closure_signature_tracked,
    normalized_expression_node, runtime_intrinsic_name_tracked, runtime_intrinsic_tracked, spawn_entry_operand,
    spawn_entry_validation_tracked, spawn_legality_tracked, spawn_stack_capture, spawn_target_tracked,
};
use layouts::{
    abi_local_declaration_type, abi_type_for_direct_aggregate_field_projection, abi_type_for_local_path,
    aggregate_field_access_tracked, aggregate_field_layout, aggregate_layout_tracked,
    aggregate_literal_declaration_tracked, aggregate_shape_from_applied_type, array_index_element_abi_type_tracked,
    contextual_enum_constructor_type_path, empty_array_literal_element_abi_type_tracked, enum_constructor_tracked,
    enum_field_layout, enum_layout_from_definition, enum_layout_substitutions, enum_layout_tracked,
    enum_match_scrutinee_layout, enum_match_tracked, enum_pattern_targets_declaration,
    instantiated_enum_layout_for_path, nominal_aggregate_abi_type, nominal_local_receiver_declaration,
    resolve_nominal_layout_declaration, resolve_type_declaration, semantic_type_from_syntax,
    unique_exported_type_in_unit, unique_public_type_in_unit, unique_type_in_unit,
};
use locals::{
    ancestor_distance, constant_integer_tracked, is_ancestor, local_declaration_is_mutable, local_declaration_owner,
    local_declaration_scope, local_slot_for_declaration, local_slot_tracked, mutable_local_assignment_tracked,
    nearest_ancestor, parent_node, resolve_lexical_declaration, resolved_local_tracked,
};
use queries::with_registered_syntax;
use resolution::{
    import_path_prefix_len, module_scope, nearest_reexport_route, outer_module_scope, public_module_routes,
    public_reexport_units, resolve_inline_module_item_declaration, resolve_item_declaration,
    resolve_item_declaration_candidate, resolve_qualified_module_unit, resolve_type_qualified_imported_function,
    resolve_unqualified_item_declaration, resolved_item_tracked, unique_exported_function_in_unit,
    unique_function_in_unit, unique_imported_function, unique_inline_module_in_scope, unique_public_function_in_unit,
};
use syntax_facts::{
    binary_operator, block_statement_nodes_tracked, child_nodes_tracked, clif_block_body_tracked,
    direct_callees_for_item, direct_callees_tracked, dispatch_builtin_symbol_tracked, item_body_tracked,
    item_export_symbol_tracked, item_name_tracked, literal_fact_tracked, node_kind_tracked, node_span_tracked,
    operator_fact_for_binary, operator_fact_tracked, reachable_items_tracked, test_bool_literal, test_item_tracked,
    test_statement_nodes_tracked, test_string_literal, unary_operator, with_node,
};
use typing::{
    element_type_for_for_iterable, enum_match_result_semantic_type, local_declaration_type, node_type_tracked,
    pattern_binding_fact, pattern_binding_semantic_type, semantic_type_for_binary_operands,
    semantic_type_for_expression, semantic_type_for_literal, semantic_type_for_local_path, semantic_type_for_node,
};

pub use abi::generic_specialization_instance;
pub use calls::extern_contract_import_for_declaration;
pub use completion::completion_candidates;
pub use model::{
    AggregateFieldAccess, AggregateFieldShape, AggregateLayoutFact, AstNodeKey, BulkParameterFact, CallLowering,
    CaptureStorage, CaptureStorageClass, CastIntent, ClosureAllocationStatus, ClosureCallTarget, ClosureCapture,
    ClosureEnvironment, ClosureEnvironmentAbiShape, ClosureEnvironmentField, ClosureLoweringStatus,
    ClosurePointerMapRequirement, ClosureSignature, CollectionMutationOwner, CollectionOperation, CompletionCandidate,
    CompletionContext, CompletionKind, ControlFlow, CorelibService, EnumConstructorFact, EnumLayoutFact,
    EnumMatchArmFact, EnumMatchBindingFact, EnumMatchFact, EnumScalarPayloadObjectLayout,
    EnumScalarPayloadVariantLayout, EnumVariantLayoutFact, ExportSymbol, ForIteratorFact, GenericCallInstantiation,
    GenericCallSpecialization, GenericCallTemplate, GenericNominalMethodReceiver, GenericSpecializationInstance,
    GenericSubstitution, IndexedNodeKind, ItemSignature, LiteralFact, LocalSlot, MutableLocalAssignment, OperatorFact,
    PrimitiveNumericConversion, RangeForFact, ResolvedItem, ResolvedLocal, RuntimeIntrinsic, RuntimeIntrinsicName,
    ScalarAbiLayout, SemanticError, SemanticQueryResult, SemanticTypeId, SourceSpan, SourceUnitId, SpawnDiagnostic,
    SpawnDiagnosticKind, SpawnEntryValidation, SpawnLegality, SpawnTarget, SyntaxUnitInput, SyntaxUnitRevision,
    TestItem, TryExpressionFact, TypedProgram, format_ast_node_key, format_ast_node_site, format_ast_node_trace,
    format_source_span_range, generic_specialization_identity,
};
pub use queries::{
    abi_type, aggregate_field_access, aggregate_layout, aggregate_literal_declaration, array_index_element_abi_type,
    binary_operand_abi_type, block_statement_nodes, bulk_parameter, call_abi_signature, call_argument_abi_type,
    call_arguments, call_lowering, callable_signature, capture_storage, cast_intents, child_nodes, clif_block_body,
    closure_call_target, closure_environment, closure_signature, collection_operation, constant_integer,
    contextual_integer_literal_abi_type, control_flow, direct_callees, empty_array_literal_element_abi_type,
    enum_constructor, enum_layout, enum_match, for_iterator_fact, generic_call_instantiation,
    generic_call_specialization, generic_call_template, generic_nominal_method_receiver, item_abi_signature, item_body,
    item_export_symbol, item_name, item_signature, literal_fact, local_slot, mutable_local_assignment, node_kind,
    node_span, node_type, nominal_member_receiver, operator_fact, primitive_numeric_conversion, range_for_fact,
    reachable_items, resolved_item, resolved_local, runtime_intrinsic, runtime_intrinsic_name, spawn_entry_validation,
    spawn_legality, spawn_target, test_item, test_statement_nodes, try_expression_fact, value_abi_type,
};
pub use syntax_facts::{DispatchBuiltinSymbol, dispatch_builtin_symbol};
