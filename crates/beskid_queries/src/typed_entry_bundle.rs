//! Debounced typed entry bundle: fast resolution vs full executable prepare.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::services::{FrontEndOptions, PrepareOptions, ResolvedInput};
use beskid_pipeline::PipelineObserver;
use salsa::Setter;

use crate::db::{BeskidDatabase, Db};
use crate::entry::{entry_resolution_with_db, fingerprint_key, prepare_compilation_with_db};
use crate::inputs::{GrammarRevision, ProjectSession};
use crate::output::{SharedFrontEnd, SharedResolution};
use crate::stats::{record_revision_bump, trace_query, trace_query_with_reason};

/// Per-entry file edit revision (bumps immediately on buffer change).
#[salsa::input]
pub struct FileRevision {
    #[returns(ref)]
    pub entry_key: String,
    pub revision: u64,
}

/// Debounced executable-prepare revision (background bump after coalesce window).
#[salsa::input]
pub struct TypedPrepareRevision {
    #[returns(ref)]
    pub entry_key: String,
    pub revision: u64,
}

/// Combined entry products for IDE hosts: resolution always available; the executable front end
/// of the prepare that last caught up with the file revision.
#[derive(Debug, Clone)]
pub struct TypedEntryState {
    pub resolution: SharedResolution,
    pub executable: Option<SharedFrontEnd>,
    /// Syntax generation owned by the prepared program assembly.
    pub generation: u64,
}

/// Typed-entry inputs and side cache of one database storage.
///
/// The revision inputs are Salsa inputs of that storage, so they live with it: a database that
/// replaces another (a reset, another project) starts with none, and databases of several
/// projects held side by side never read each other's inputs.
#[derive(Default)]
pub struct TypedEntryRegistry {
    file_revisions: HashMap<String, FileRevision>,
    typed_prepare_revisions: HashMap<String, TypedPrepareRevision>,
    /// Full executable prepare (keyed by debounced `typed_prepare_revision`).
    full_bundles: HashMap<String, (u64, SharedFrontEnd)>,
    /// EntryOnly gate prepare while file edits are ahead of debounced prepare (keyed by `file_revision`).
    gate_bundles: HashMap<String, (u64, SharedFrontEnd)>,
}

fn typed_entry_registry(db: &dyn Db) -> std::sync::MutexGuard<'_, TypedEntryRegistry> {
    db.typed_entry_registry().lock().expect("typed entry registry")
}

fn ensure_file_revision(db: &mut BeskidDatabase, entry_key: &str) -> FileRevision {
    if let Some(revision) = typed_entry_registry(db).file_revisions.get(entry_key) {
        return *revision;
    }
    let revision = FileRevision::new(db, entry_key.to_string(), 0);
    typed_entry_registry(db).file_revisions.insert(entry_key.to_string(), revision);
    revision
}

fn ensure_typed_prepare_revision(db: &mut BeskidDatabase, entry_key: &str) -> TypedPrepareRevision {
    if let Some(revision) = typed_entry_registry(db).typed_prepare_revisions.get(entry_key) {
        return *revision;
    }
    let revision = TypedPrepareRevision::new(db, entry_key.to_string(), 0);
    typed_entry_registry(db).typed_prepare_revisions.insert(entry_key.to_string(), revision);
    revision
}

/// Bump file revision immediately after an edit (fast resolution invalidation).
pub fn bump_file_revision(db: &mut BeskidDatabase, entry_key: &str) -> u64 {
    let revision = ensure_file_revision(db, entry_key);
    let next = revision.revision(db).saturating_add(1);
    revision.set_revision(db).to(next);
    record_revision_bump();
    typed_entry_registry(db).gate_bundles.remove(entry_key);
    next
}

/// Bump typed-prepare revision after debounce (triggers full executable prepare).
pub fn bump_typed_prepare_revision(db: &mut BeskidDatabase, entry_key: &str) -> u64 {
    let revision = ensure_typed_prepare_revision(db, entry_key);
    let next = revision.revision(db).saturating_add(1);
    revision.set_revision(db).to(next);
    record_revision_bump();
    clear_typed_entry_cache_for_entry(db, entry_key);
    next
}

pub fn file_revision_for(db: &dyn Db, entry_key: &str) -> u64 {
    let revision = typed_entry_registry(db).file_revisions.get(entry_key).copied();
    revision.map_or(0, |revision| revision.revision(db))
}

pub fn typed_prepare_revision_for(db: &dyn Db, entry_key: &str) -> u64 {
    let revision = typed_entry_registry(db).typed_prepare_revisions.get(entry_key).copied();
    revision.map_or(0, |revision| revision.revision(db))
}

/// True when file edits have not yet been reflected in a typed prepare pass.
pub fn is_typed_bundle_stale(db: &dyn Db, entry_key: &str) -> bool {
    file_revision_for(db, entry_key) != typed_prepare_revision_for(db, entry_key)
}

fn prepare_options_fingerprint(options: &PrepareOptions) -> String {
    format!(
        "discovery={:?}:recovery={:?}:semantic={}",
        options.front_end.assembly_discovery,
        options.front_end.assembly_recovery,
        options.front_end.with_semantic_diagnostics,
    )
}

fn project_session_for_resolved(db: &mut BeskidDatabase, resolved: &ResolvedInput) -> Option<ProjectSession> {
    let plan = resolved.compile_plan.as_ref()?;
    let lockfile_digest = lockfile_digest_for_plan(plan);
    Some(db.ensure_project_session(plan, &resolved.source_path, lockfile_digest))
}

pub(crate) fn lockfile_digest_for_plan(plan: &beskid_analysis::projects::CompilePlan) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    plan.project_root.hash(&mut hasher);
    plan.target.entry.hash(&mut hasher);
    plan.target.name.hash(&mut hasher);
    if let Ok(bytes) = std::fs::read(plan.project_root.join("Project.lock")) {
        bytes.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Executable prepare for the typed bundle. The caller's qualified Mod invoker is authoritative:
/// every Corelib project runs the Corelib Mods (the Serialization Mod), so dropping it would make
/// preparation fail closed on a missing Mod host even when the caller supplied one.
fn materialize_typed_bundle(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let prepared = prepare_compilation_with_db(
        db,
        resolved,
        PrepareOptions {
            mod_host: options.mod_host.clone(),
            front_end: FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
            native_mod_adapter_sources: false,
        },
        pipeline,
    )?;
    Ok(SharedFrontEnd(Arc::new(prepared.into_executable()?)))
}

/// Salsa memoization boundary for executable prepare (heavy `SharedFrontEnd` lives in side cache).
#[salsa::tracked]
pub fn typed_entry_bundle_tracked(
    db: &dyn Db,
    project: ProjectSession,
    grammar: GrammarRevision,
    entry_key: String,
    typed_prepare_revision: u64,
    options_fingerprint: String,
) -> u64 {
    let _ = (db, entry_key, project, grammar, options_fingerprint);
    trace_query("typed_entry_bundle_tracked", false);
    typed_prepare_revision
}

fn run_tracked_typed_prepare(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    entry_key: &str,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let typed_prepare_revision = typed_prepare_revision_for(db, entry_key);
    let cache_hit = typed_entry_registry(db)
        .full_bundles
        .get(entry_key)
        .is_some_and(|(generation, _)| *generation == typed_prepare_revision);
    trace_query_with_reason(
        "typed_entry_bundle",
        cache_hit,
        if cache_hit { Some("typed_prepare_revision") } else { None },
    );

    let Some(project) = project_session_for_resolved(db, resolved) else {
        return materialize_typed_bundle(db, resolved, options, pipeline);
    };
    let grammar = db.grammar_revision();
    let options_fingerprint = prepare_options_fingerprint(options);
    let _ = typed_entry_bundle_tracked(
        db,
        project,
        grammar,
        entry_key.to_string(),
        typed_prepare_revision,
        options_fingerprint,
    );

    if let Some((generation, front)) = typed_entry_registry(db)
        .full_bundles
        .get(entry_key)
        .filter(|(generation, _)| *generation == typed_prepare_revision)
        .map(|(generation, front)| (*generation, front.clone()))
    {
        let _ = generation;
        return Ok(front);
    }

    let front = materialize_typed_bundle(db, resolved, options, pipeline)?;
    typed_entry_registry(db).full_bundles.insert(entry_key.to_string(), (typed_prepare_revision, front.clone()));
    Ok(front)
}

fn run_tracked_entry_gate_prepare(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    entry_key: &str,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let file_revision = file_revision_for(db, entry_key);
    let cache_hit = typed_entry_registry(db)
        .gate_bundles
        .get(entry_key)
        .is_some_and(|(generation, _)| *generation == file_revision);
    trace_query_with_reason("typed_entry_gate_bundle", cache_hit, if cache_hit { Some("file_revision") } else { None });

    if let Some((generation, front)) = typed_entry_registry(db)
        .gate_bundles
        .get(entry_key)
        .filter(|(generation, _)| *generation == file_revision)
        .map(|(generation, front)| (*generation, front.clone()))
    {
        let _ = generation;
        return Ok(front);
    }

    let front = materialize_typed_bundle(db, resolved, options, pipeline)?;
    typed_entry_registry(db).gate_bundles.insert(entry_key.to_string(), (file_revision, front.clone()));
    Ok(front)
}

/// Debounced entry bundle: fast resolution always; the executable front end of the gate prepare
/// while file edits are ahead of the debounced prepare revision, else of the full prepare.
pub fn typed_entry_state_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<TypedEntryState> {
    let entry_key = crate::entry::session_fingerprint(resolved)
        .map(|fp| fingerprint_key(&fp))
        .unwrap_or_else(|| resolved.source_path.display().to_string());
    let file_revision = file_revision_for(db, &entry_key);
    let typed_prepare_revision = typed_prepare_revision_for(db, &entry_key);
    let stale = file_revision != typed_prepare_revision;

    let resolution = entry_resolution_with_db(db, resolved, options)?;
    let typed = if stale {
        run_tracked_entry_gate_prepare(db, resolved, &entry_key, options, pipeline)?
    } else {
        run_tracked_typed_prepare(db, resolved, &entry_key, options, pipeline)?
    };
    let generation = typed.assembly.generation.0;
    Ok(TypedEntryState { resolution, executable: Some(typed), generation })
}

/// Executable typed bundle when prepare revision caught up; otherwise runs full prepare.
pub fn typed_entry_bundle_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: &PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let state = typed_entry_state_with_db(db, resolved, options, pipeline)?;
    if let Some(executable) = state.executable {
        return Ok(executable);
    }
    run_tracked_typed_prepare(
        db,
        resolved,
        &crate::entry::session_fingerprint(resolved)
            .map(|fp| fingerprint_key(&fp))
            .unwrap_or_else(|| resolved.source_path.display().to_string()),
        options,
        pipeline,
    )
}

pub fn clear_typed_entry_cache_for_entry(db: &dyn Db, entry_key: &str) {
    let mut registry = typed_entry_registry(db);
    registry.full_bundles.remove(entry_key);
    registry.gate_bundles.remove(entry_key);
}
