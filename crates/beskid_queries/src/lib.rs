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
mod session;
mod stats;
mod typed_entry_bundle;
mod typed_program;
mod unit;

pub use beskid_analysis::syntax::{AstNodeId, SyntaxGenerationId};
pub use beskid_graph::GraphKind;
pub use db::{
    BeskidDatabase, Db, UnitArtifactCache, configure_compilation_database_for_project,
    replace_compilation_database, reset_compilation_database,
};
pub use entry::{
    cached_semantic_snapshot_for_key, entry_resolution_with_db, fingerprint_key,
    invalidate_entry_sessions, prepare_compilation_diagnostics_with_db,
    prepare_compilation_with_db, semantic_gate_diagnostics, semantic_snapshot, session_fingerprint,
    typed_entry_bundle,
};
pub use graph::{
    discovered_units, module_index_fingerprint, program_assembly, program_assembly_tracked,
    reverse_dependents,
};
pub use graph_viz::{
    GraphFetchRequest, GraphQueryError, get_graph_document, get_graph_document_simple,
    graph_fingerprint_project_deps, graph_mermaid_project_deps, graph_mermaid_workspace,
    manifest_digest,
};
pub use inputs::{FileText, GrammarRevision, ProjectSession};
pub use modhost::{
    CapabilitySetId, ManifestGenerationId, ModHostSyntaxGenerationId, bump_syntax_generation,
    mod_collect_target_fingerprint, mod_generate, mod_generate_fingerprint,
};
pub use output::{
    SharedFrontEnd, SharedResolution, SharedTypeResult, SharedUnitResolution, SharedUnitTypeSurface,
};
pub use persistence::{
    SalsaPersistenceManifest, cache_root_for_project, ensure_salsa_dir, load_manifest,
};
pub use semantic_contract::{
    AggregateFieldAccess, AggregateFieldShape, AggregateLayoutFact, AstNodeKey, CallLowering,
    CastIntent, ClosureCapture, ClosureEnvironment, CompletionCandidate, CompletionContext,
    CompletionKind, ControlFlow, CorelibService, EnumConstructorFact, EnumLayoutFact,
    EnumMatchArmFact, EnumMatchFact, EnumVariantLayoutFact, ExportSymbol, GenericCallInstantiation,
    GenericCallSpecialization, IndexedNodeKind, ItemSignature, LiteralFact, LocalSlot,
    OperatorFact, ResolvedItem, ResolvedLocal, RuntimeIntrinsic, RuntimeIntrinsicName,
    SemanticError, SemanticQueryResult, SemanticTypeId, SourceSpan, SourceUnitId, SpawnTarget,
    format_ast_node_key,
    TestItem, TypedProgram, abi_type, aggregate_field_access, aggregate_layout,
    aggregate_literal_declaration, block_statement_nodes, call_abi_signature,
    call_argument_abi_type, call_arguments, call_lowering, cast_intents, child_nodes,
    closure_environment, completion_candidates, control_flow, direct_callees, enum_constructor,
    enum_layout, enum_match, generic_call_instantiation, generic_call_specialization,
    item_abi_signature, item_body, item_export_symbol, item_name, item_signature, literal_fact,
    local_slot, node_kind, node_span, node_type, nominal_member_receiver, operator_fact,
    dispatch_builtin_symbol, DispatchBuiltinSymbol,
    reachable_items, resolved_item, resolved_local, runtime_intrinsic, runtime_intrinsic_name,
    spawn_target, test_item, test_statement_nodes,
};
pub use session::{
    compile_front_end_from_resolved_input, configure_db_for_project, prepare_compilation,
    prepare_compilation_diagnostics, with_db,
};
pub use stats::{
    emit_salsa_stats, record_query_hit, record_query_miss, record_revision_bump, reset, snapshot,
};
pub use typed_entry_bundle::{
    FileRevision, TypedEntryState, TypedPrepareRevision, bump_file_revision,
    bump_typed_prepare_revision, clear_typed_entry_cache, file_revision_for, is_typed_bundle_stale,
    reset_typed_entry_inputs, typed_entry_bundle_tracked, typed_entry_bundle_with_db,
    typed_entry_state_with_db, typed_prepare_revision_for,
};
pub use typed_program::build_canonical_corelib_syscall_typed_program;
pub use typed_program::build_canonical_runtime_typed_program;
pub use typed_program::build_typed_program;
pub use typed_program::build_typed_program_with_corelib_services;
pub use typed_program::build_typed_program_with_corelib_syscall_services;
pub use typed_program::project_session_for_syntax_assembly;
pub use unit::{
    parse_and_expand_unit, parse_and_expand_unit_tracked, parse_and_expand_unit_with_source,
    seed_file_from_disk, unit_content_fingerprint, unit_imports,
};

pub use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, PrepareOptions, PreparedCompilation,
};
