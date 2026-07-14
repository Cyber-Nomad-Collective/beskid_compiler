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
    AstNodeKey, CallLowering, CastIntent, ControlFlow, IndexedNodeKind, ItemSignature, LiteralFact,
    LocalSlot, OperatorFact, ResolvedItem, ResolvedLocal, RuntimeIntrinsic, SemanticError,
    SemanticQueryResult, SemanticTypeId, SourceSpan, SourceUnitId, TypedProgram, call_arguments,
    call_lowering, cast_intents, child_nodes, control_flow, direct_callees, item_body,
    item_signature, literal_fact, local_slot, node_kind, node_span, node_type, operator_fact,
    reachable_items, resolved_item, resolved_local, runtime_intrinsic,
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
pub use typed_program::build_typed_program;
pub use unit::{
    cache_module_index_for_assembly, module_index_fingerprint_for_assembly, parse_and_expand_unit,
    parse_and_expand_unit_tracked, parse_and_expand_unit_with_source, seed_file_from_disk,
    unit_content_fingerprint, unit_hir, unit_hir_tracked, unit_hir_with_source, unit_imports,
    unit_resolution, unit_resolution_tracked, unit_type_surface, unit_type_surface_tracked,
    warm_prefetched_unit_type_surfaces,
};

pub use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, PrepareOptions, PreparedCompilation,
};
