//! Mod-host incremental query group (spec invalidation keys).

use beskid_analysis::mod_host::{ModHostInput, run_through_generate};

use crate::db::Db;
use crate::inputs::{FileText, ProjectSession};
use crate::stats::record_query_miss;

/// Spec: `syntax_generation_id` — bumped when entry file text changes.
#[salsa::input]
pub struct SyntaxGenerationId {
    pub path: String,
    pub generation: u64,
}

/// Spec: `manifest_generation_id` — hash of manifest/lockfile bytes.
#[salsa::input]
pub struct ManifestGenerationId {
    #[returns(ref)]
    pub digest: String,
}

/// Spec: `capability_set_id` — canonical mod capability grant encoding.
#[salsa::input]
pub struct CapabilitySetId {
    #[returns(ref)]
    pub digest: String,
}

/// Fingerprint of mod-generate inputs (tracked).
#[salsa::tracked]
pub fn mod_generate_fingerprint(
    db: &dyn Db,
    project: ProjectSession,
    entry: FileText,
    syntax_gen: SyntaxGenerationId,
    manifest_gen: ManifestGenerationId,
) -> String {
    let _ = (project, syntax_gen, manifest_gen);
    record_query_miss();
    format!("{}:{}", entry.path(db).display(), entry.text(db).len())
}

/// Run mod generate when fingerprint changes; returns source length as cheap tracked output.
#[salsa::tracked]
pub fn mod_generate(
    db: &dyn Db,
    project: ProjectSession,
    entry: FileText,
    syntax_gen: SyntaxGenerationId,
    manifest_gen: ManifestGenerationId,
) -> u64 {
    let _ = mod_generate_fingerprint(db, project, entry, syntax_gen, manifest_gen);
    record_query_miss();
    let source_name = entry.path(db).display().to_string();
    let source = entry.text(db).clone();
    let program = beskid_analysis::services::parse_program_with_source_name(&source_name, &source)
        .expect("entry parse");
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: None,
            source_name: &source_name,
            source: &source,
            pipeline: None,
            invoker: None,
        },
    )
    .expect("mod generate");
    generated.program.node.items.len() as u64
}

/// Bump syntax generation counter for a path (call after file edit).
pub fn bump_syntax_generation(db: &mut crate::db::BeskidDatabase, path: &str, generation: u64) {
    let _ = SyntaxGenerationId::new(db, path.to_string(), generation);
}
