//! Mod-host incremental query group (spec invalidation keys).

use beskid_analysis::mod_host::{
    ModHostInput, collect_mod_target_fingerprint, native_invoker_for_plan, run_through_generate,
};
use beskid_analysis::projects::{
    build_compile_plan, discover_project_manifest_in_dir,
};

use crate::db::Db;
use crate::inputs::{FileText, ProjectSession};
use crate::stats::record_query_miss;

/// Spec: `syntax_generation_id` — bumped when entry file text changes.
#[salsa::input]
pub struct ModHostSyntaxGenerationId {
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

/// Fingerprint of collector-observed mod targets (tracked).
#[salsa::tracked]
pub fn mod_collect_target_fingerprint(
    db: &dyn Db,
    project: ProjectSession,
    entry: FileText,
    syntax_gen: ModHostSyntaxGenerationId,
    manifest_gen: ManifestGenerationId,
    capability_set: CapabilitySetId,
) -> String {
    let _ = (project, entry, syntax_gen, manifest_gen, capability_set);
    record_query_miss();
    let source_name = entry.path(db).display().to_string();
    let source = entry.text(db).clone();
    let compile_plan = compile_plan_for_session(db, project);
    let native_invoker = compile_plan
        .as_ref()
        .and_then(|plan| native_invoker_for_plan(plan, None).ok().flatten());
    let invoker_ref = native_invoker
        .as_ref()
        .map(|invoker| invoker as &dyn beskid_analysis::mod_host::ContractInvoker);
    collect_mod_target_fingerprint(&ModHostInput {
        compile_plan: compile_plan.as_ref(),
        source_name: &source_name,
        source: &source,
        pipeline: None,
        invoker: invoker_ref,
        cached_target_fingerprint: None,
    })
    .unwrap_or_default()
}

/// Fingerprint of mod-generate inputs (tracked).
#[salsa::tracked]
pub fn mod_generate_fingerprint(
    db: &dyn Db,
    project: ProjectSession,
    entry: FileText,
    syntax_gen: ModHostSyntaxGenerationId,
    manifest_gen: ManifestGenerationId,
    _capability_set: CapabilitySetId,
    collect_targets: String,
) -> String {
    let _ = project;
    record_query_miss();
    format!(
        "{}:{}:{}:{}:{}",
        entry.path(db).display(),
        entry.text(db).len(),
        syntax_gen.generation(db),
        manifest_gen.digest(db),
        collect_targets
    )
}

/// Run mod host generate phase when fingerprint changes; returns source length as cheap tracked output.
#[salsa::tracked]
pub fn mod_generate(
    db: &dyn Db,
    project: ProjectSession,
    entry: FileText,
    syntax_gen: ModHostSyntaxGenerationId,
    manifest_gen: ManifestGenerationId,
    capability_set: CapabilitySetId,
    collect_targets: String,
) -> u64 {
    let _ = mod_generate_fingerprint(
        db,
        project,
        entry,
        syntax_gen,
        manifest_gen,
        capability_set,
        collect_targets.clone(),
    );
    record_query_miss();
    let source_name = entry.path(db).display().to_string();
    let source = entry.text(db).clone();
    let program = beskid_analysis::services::parse_program_with_source_name(&source_name, &source)
        .expect("entry parse");
    let compile_plan = compile_plan_for_session(db, project);
    let native_invoker = compile_plan
        .as_ref()
        .and_then(|plan| native_invoker_for_plan(plan, None).ok().flatten());
    let invoker_ref = native_invoker
        .as_ref()
        .map(|invoker| invoker as &dyn beskid_analysis::mod_host::ContractInvoker);
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: compile_plan.as_ref(),
            source_name: &source_name,
            source: &source,
            pipeline: None,
            invoker: invoker_ref,
            cached_target_fingerprint: None,
        },
    )
    .expect("mod host generate");
    generated.program.node.items.len() as u64
}

fn compile_plan_for_session(
    db: &dyn Db,
    project: ProjectSession,
) -> Option<beskid_analysis::projects::CompilePlan> {
    let root = project.project_root(db);
    let manifest_path = discover_project_manifest_in_dir(&root).ok()??;
    build_compile_plan(&manifest_path, None).ok()
}

/// Bump syntax generation counter for a path (call after file edit).
pub fn bump_syntax_generation(db: &mut crate::db::BeskidDatabase, path: &str, generation: u64) {
    let _ = ModHostSyntaxGenerationId::new(db, path.to_string(), generation);
}
