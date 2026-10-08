//! Salsa incremental query database for Beskid compilation.
//!
//! Syntax revision authority is intentionally opaque to callers:
//!
//! ```compile_fail
//! use beskid_queries::SyntaxUnitInput;
//! ```
//!
//! ```compile_fail
//! use beskid_queries::Db;
//!
//! fn replace_registered_authority(db: &dyn Db) {
//!     db.syntax_unit_registry().lock().unwrap().clear();
//! }
//! ```
//!
//! ```compile_fail
//! use std::path::PathBuf;
//! use beskid_queries::{BeskidDatabase, SourceUnitId, SyntaxGenerationId};
//!
//! let mut db = BeskidDatabase::default();
//! let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd"));
//! let authority = db.ensure_syntax_unit(project, unit, SyntaxGenerationId(1))?;
//! let _ = authority.set_generation(&mut db);
//! ```

mod db;
mod mod_semantic_authority;
mod mod_semantic_scope;
mod mod_syntax_authority;
pub use mod_semantic_authority::{
    CompiledCatchallBinding, ReboundCatchallBinding, CompiledSerializationTarget, SerializationContributionKind, ModSemanticCatchall, ModSemanticQueryAuthority,
    CompiledSerializationTemplate, serialization_template_getter_shape,
};
pub use semantic_contract::{
    canonical_container_kind, CanonicalContainerKind, DynamicPackingBridge, DynamicPackingField, DynamicPackingNode, DynamicPackingShape, DynamicPackingVariant,
    DynamicShapeBinding, DynamicUnpackingBridge, dynamic_packing_bridge, dynamic_packing_shape, dynamic_shape_binding,
    dynamic_unpacking_bridge,
};
mod entry;
mod expand;
mod graph;
mod graph_viz;
mod inputs;
mod materializer;
mod modhost;
mod output;
mod persistence;
mod semantic_contract;
pub use semantic_contract::{
    CheckedFailureDestination, ReservedFailureConstructor, canonical_serialization_encode, checked_failure_destination,
    reserved_failure_constructor, serialization_publication_methods,
};
mod session;
mod stats;
mod typed_entry_bundle;
mod typed_program;
mod unit;

pub use beskid_analysis::syntax::{AstNodeId, SyntaxGenerationId};
pub use beskid_graph::GraphKind;
pub use db::{
    BeskidDatabase, Db, UnitArtifactCache, configure_compilation_database_for_project, replace_compilation_database,
    reset_compilation_database,
};
pub use entry::prepare_compilation_diagnostics_isolated;
pub use entry::{
    assemble_resolved_input_with_db, cached_semantic_snapshot_for_key, entry_resolution_with_db, fingerprint_key,
    invalidate_entry_sessions, prepare_compilation_diagnostics_with_db, prepare_compilation_with_db,
    semantic_diagnostics_for_roots, semantic_gate_diagnostics, semantic_snapshot, session_fingerprint,
    typed_entry_bundle,
};
pub use graph::{
    discovered_units, module_index_fingerprint, program_assembly, program_assembly_tracked, reverse_dependents,
};
pub use graph_viz::{
    GraphFetchRequest, GraphQueryError, get_graph_document, get_graph_document_simple, graph_fingerprint_project_deps,
    graph_mermaid_project_deps, graph_mermaid_workspace, manifest_digest,
};
pub use inputs::{FileText, GrammarRevision, ProjectSession};
pub use modhost::{ManifestGenerationId, ModHostSyntaxGenerationId};
pub use output::{SharedFrontEnd, SharedResolution, SharedTypeResult, SharedUnitResolution, SharedUnitTypeSurface};
pub use persistence::{
    SalsaPersistenceManifest, cache_root_for_project, ensure_salsa_dir, load_db_snapshot, load_manifest,
    persist_session_snapshot, save_db_snapshot,
};
pub use semantic_contract::{
    AggregateFieldAccess, AggregateFieldShape, AggregateLayoutFact, AggregateLiteralFieldValues,
    ArrayIndexElementTemplate, AstNodeKey, BulkParameterFact, CallLowering, CaptureStorageClass, CastIntent,
    ClosureAllocationStatus, ClosureCallTarget, ClosureCapture, ClosureEnvironment, ClosureEnvironmentField,
    ClosureLoweringStatus, ClosurePointerMapRequirement, CollectionMutationOwner, CollectionOperation,
    CompletionCandidate, CompletionContext, CompletionKind, CompletionMemberSurface, ControlFlow, CorelibService,
    EnumConstructorFact, EnumConstructorSpecialization, EnumConstructorTemplate, EnumLayoutFact,
    EnumLayoutTemplateArgument, EnumMatchArmFact, EnumMatchBindingFact, EnumMatchFact, EnumMatchPatternFact,
    EnumMatchScalarLiteralFact, EnumMatchVariantPatternFact, EnumScalarPayloadObjectLayout,
    EnumScalarPayloadVariantLayout, EnumVariantLayoutFact, EventFieldLayoutFact, EventHandlerLocalFact,
    EventOperationFact, EventOperationKind, ExportSymbol, FiberOwnership, ForIteratorFact, GenericCallInstantiation,
    GenericCallSpecialization, GenericCallTemplate, GenericNominalMethodReceiver, GenericSpecializationInstance,
    GenericSubstitution, IndexedNodeKind, ItemSignature, LiteralFact, LocalSlot, ManagedReferenceKind, ManifestBuiltin,
    MutableLocalAssignment, NativeModCallback, NativeModCallbackOperation, NativeModContractDeclaration,
    NativeModContractFamily, NativeModRequestConstructor, NativeModRequestFactory, NativeModSyntaxConstructor,
    NativeModTransportBody, NativeModTransportField, NativeModTransportNominal, NativeModTransportSignature,
    NativeModTransportType, NativeModTransportVariant, OperatorFact, PrimitiveNumericConversion, RangeForFact,
    ResolvedItem, ResolvedLocal, RuntimeIntrinsic, RuntimeIntrinsicName, ScalarAbiLayout, ScopedAcquisition,
    ScopedCleanup, ScopedCleanupDiagnostic, SemanticError, SemanticQueryResult, SemanticTypeId, SourceSpan,
    SourceUnitId, SpawnDiagnosticKind, SpawnEntryValidation, SpawnTarget, TestItem, TryExpressionFact,
    TypedArrayAllocation, TypedProgram, abi_type, aggregate_field_access, aggregate_field_access_specialization,
    aggregate_layout, aggregate_literal_declaration, aggregate_literal_field_values, aggregate_literal_layout,
    aggregate_literal_specialization, array_index_element_abi_type, array_index_element_specialization,
    binary_operand_abi_type, block_statement_nodes, bulk_parameter, call_abi_signature, call_argument_abi_type,
    call_arguments, call_lowering, callable_fiber_ownership, callable_signature, capture_storage, cast_intents,
    child_nodes, clif_block_body, closure_call_target, closure_environment, closure_signature, collection_operation,
    completion_candidates, completion_dependency_surface, completion_dependency_surface_for_assembly,
    completion_dependency_surface_for_program, constant_integer, contextual_integer_literal_abi_type, control_flow,
    direct_callees, empty_array_literal_element_abi_type, empty_array_literal_element_specialization, enum_constructor,
    enum_constructor_specialization, enum_constructor_template, enum_layout, enum_match, enum_match_specialization,
    event_field_layout, event_handler_lambda_for_local, event_operation, extern_contract_import_for_declaration,
    for_iterator_fact, format_ast_node_key, format_ast_node_site, format_ast_node_trace, format_source_span_range,
    generic_call_instantiation, generic_call_specialization, generic_call_specialization_in_environment,
    generic_call_specialization_instance, generic_call_template, generic_nominal_method_receiver,
    generic_specialization_identity, generic_specialization_instance, implicit_method_receiver, item_abi_signature,
    item_body, item_export_symbol, item_name, item_signature, literal_fact, local_slot, managed_reference_kind,
    mutable_local_assignment, native_mod_callback_symbol, native_mod_contract_declaration,
    native_mod_request_constructor, native_mod_syntax_constructor, native_mod_transport_nominal,
    native_mod_transport_signature, node_kind, node_span, node_type, nominal_member_receiver, operator_fact,
    parameter_generic_reference, pattern_binding_specialization, primitive_numeric_conversion, range_for_fact,
    reachable_items, resolved_item, resolved_local, runtime_intrinsic, runtime_intrinsic_name, scoped_cleanup,
    spawn_entry_validation, spawn_handle_type, spawn_legality, spawn_target,
    specialized_call_result_managed_reference_kind, specialized_corelib_value_service_result, test_item,
    test_statement_nodes, try_expression_fact, typed_array_allocation, value_abi_type,
};
pub use semantic_contract::{
    AppliedContractIdentity, applied_contract_argument_is_type, type_applied_contract_implementation,
    type_contract_applications, type_contract_applications_of, type_contract_declarations,
    type_contract_implementation,
};
pub use semantic_contract::{
    CallArityMismatch, GateObligation, GateObligationKind, GenericBindingConflict, GenericBoundViolation,
    GenericParameterConflict, ImmutableLocalAssignment, MemberReferenceFinding, MemberReferenceKind,
    NonExhaustiveMatch, SemanticFinding, TypingObligation, TypingObligationKind, UnitObligation, UnitObligationKind,
    UnresolvedCallKind, UnresolvedCallTarget, UnresolvedImport, UnresolvedValueArgument, ValueObligation,
    call_arity_mismatch,
    check_gate_obligations, check_items, check_typing_obligations, check_unit_obligations, extern_profile_findings,
    gate_obligations, generic_parameter_conflict, immutable_local_assignment, match_exhaustiveness,
    member_reference_legality, typing_obligations, unit_obligations, unresolved_call_target, unresolved_imports,
    unresolved_value_argument, value_obligations,
};
pub use semantic_contract::{
    CompositionInjectedAccessFact, CompositionInjectionFieldFact, CompositionLaunchFact, CompositionRegistrationFact,
    CompositionScopeFact, composition_injected_field_access, composition_injection_field, composition_launch,
    composition_registration, composition_scope,
};
pub use semantic_contract::{DeadCollectionGrowth, dead_collection_growth, is_growth_call_candidate};
pub use semantic_contract::{UnresolvedTypeReference, unresolved_type_reference};
pub use session::{
    compile_front_end_from_resolved_input, configure_db_for_project, prepare_compilation,
    prepare_compilation_diagnostics, reset_process_compilation_database, with_db,
};
pub use stats::{emit_salsa_stats, record_query_hit, record_query_miss, record_revision_bump, reset, snapshot};
pub use typed_entry_bundle::{
    FileRevision, TypedEntryState, TypedPrepareRevision, bump_file_revision, bump_typed_prepare_revision,
    clear_typed_entry_cache, file_revision_for, is_typed_bundle_stale, reset_typed_entry_inputs,
    typed_entry_bundle_tracked, typed_entry_bundle_with_db, typed_entry_state_with_db, typed_prepare_revision_for,
};
pub use typed_program::build_canonical_runtime_typed_program;
pub use typed_program::build_runtime_fixture_typed_program;
pub use typed_program::build_typed_program;
pub use typed_program::build_typed_program_with_corelib_services;
pub use typed_program::build_typed_program_with_corelib_syscall_services;
pub use typed_program::project_session_for_planned_syntax_assembly;
pub use typed_program::project_session_for_syntax_assembly;
pub use unit::{
    parse_and_expand_unit, parse_and_expand_unit_tracked, parse_and_expand_unit_with_source, seed_file_from_disk,
    unit_content_fingerprint, unit_imports,
};

pub use beskid_analysis::services::{FrontEndOptions, FrontEndTypedResult, PrepareOptions, PreparedCompilation};

pub use semantic_contract::{
    GlueBindingFact, GlueDirection, GlueHandleBrand, GlueLogicalType, GlueOwnerBindingDigest, GlueParameterFact,
    GlueRustOwnerCallable, RustOwnerCallableRow, RustOwnerDeclaration, RustOwnerPlacement, RustOwnerTables,
    RustOwnerTypeRow, glue_binding, glue_handle_shape, rust_owner_declarations, rust_owner_tables,
    validate_rust_owner_path,
};

mod corelib_source_authority;
pub use corelib_source_authority::{canonical_corelib_source_path, registered_declaration_name};
mod managed_opaque;
pub use managed_opaque::{RuntimeManagedOpaqueKind, runtime_managed_opaque_kind};

pub use semantic_contract::{CheckedProviderCall, checked_provider_call};

pub use semantic_contract::{native_mod_expression_payload, native_mod_single_array_element};

pub use semantic_contract::{PortableNominalIdentity, portable_nominal_identity};

pub use semantic_contract::{CheckedDynamicResultBridge, checked_dynamic_result_bridge};

pub use mod_semantic_authority::ReboundSerializationTarget;

pub mod process_source_authority;

pub use semantic_contract::{SerializationShapeBinding, serialization_shape_binding};

mod serialization_field_policy;
pub use serialization_field_policy::{
    SerializationFieldPolicy, serialization_field_policy, serialization_payload_field_policy,
};
