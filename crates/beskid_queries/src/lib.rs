//! Salsa incremental query database for Beskid compilation.

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
mod session;
mod stats;
mod unit;

pub use db::{BeskidDatabase, Db, UnitArtifactCache};
pub use entry::{
    cached_semantic_snapshot_for_key, fingerprint_key, invalidate_entry_sessions,
    prepare_compilation_diagnostics_with_db, prepare_compilation_with_db, semantic_gate_diagnostics,
    semantic_snapshot, typed_entry_bundle,
};
pub use session::{
    compile_front_end_from_resolved_input, configure_db_for_project, prepare_compilation,
    prepare_compilation_diagnostics, with_db,
};
pub use graph::{assemble_program_query, discovered_units, module_index_fingerprint, program_assembly, reverse_dependents};
pub use graph_viz::{
    get_graph_document, get_graph_document_simple, graph_fingerprint_project_deps,
    graph_mermaid_project_deps, graph_mermaid_workspace, manifest_digest, GraphFetchRequest,
    GraphQueryError,
};
pub use beskid_graph::GraphKind;
pub use inputs::{FileText, ProjectSession};
pub use modhost::{
    bump_syntax_generation, mod_generate, mod_generate_fingerprint, CapabilitySetId,
    ManifestGenerationId, SyntaxGenerationId,
};
pub use output::{SharedFrontEnd, SharedResolution, SharedTypeResult};
pub use persistence::{
    cache_root_for_project, ensure_salsa_dir, load_manifest, SalsaPersistenceManifest,
};
pub use stats::{emit_salsa_stats, record_query_hit, record_query_miss, record_revision_bump, reset, snapshot};
pub use unit::{parse_and_expand_unit, seed_file_from_disk, unit_content_fingerprint, unit_hir, unit_imports};

pub use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, PrepareMode, PrepareOptions, PreparedCompilation,
};
