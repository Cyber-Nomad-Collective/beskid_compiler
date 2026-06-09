//! Graph-level queries: discovery, module index, program assembly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::ProgramAssembly;
use beskid_analysis::projects::model::AssemblyOptions;
use beskid_analysis::projects::{AssemblyError, assemble_program_with_materializer};
use beskid_analysis::projects::{CompilePlan, PreparedProjectWorkspace};

use crate::db::{BeskidDatabase, Db};
use crate::inputs::ProjectSession;
use crate::materializer::unit_materializer_for;
use crate::stats::{SALSA_TRACE_TARGET, trace_query, trace_query_with_reason};
use crate::unit::{seed_file_from_disk, unit_imports};

/// Discovered unit paths for an entry (query boundary marker).
pub fn discovered_units(
    _db: &dyn Db,
    _project: ProjectSession,
    _entry_path: PathBuf,
) -> Vec<String> {
    trace_query("discovered_units", false);
    Vec::new()
}

/// Module index fingerprint for a unit set (query boundary marker).
pub fn module_index_fingerprint(_db: &dyn Db, _project: ProjectSession, unit_count: u64) -> String {
    trace_query("module_index_fingerprint", false);
    format!("modules:{unit_count}")
}

/// BFS invalidation helper: units that depend on `changed` via import edges.
pub fn reverse_dependents(
    db: &dyn Db,
    project: ProjectSession,
    changed_path: PathBuf,
    candidate_paths: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let trigger_file = changed_path.display().to_string();
    let _bfs_guard = tracing::debug_span!(
        target: SALSA_TRACE_TARGET,
        "invalidate_import_dependents.bfs",
        trigger_file = %trigger_file,
        candidate_count = candidate_paths.len(),
    )
    .entered();

    let changed_key = trigger_file.clone();
    let mut invalidated = vec![changed_path.clone()];
    let mut queue = vec![changed_path];
    while let Some(current) = queue.pop() {
        let current_key = current.display().to_string();
        let grammar = db.grammar_revision_input();
        for path in &candidate_paths {
            let imports = unit_imports(db, project, grammar, path.clone());
            if imports
                .iter()
                .any(|dep| dep == &current_key || dep.ends_with(&current_key))
                && !invalidated.iter().any(|p| p == path)
            {
                invalidated.push(path.clone());
                queue.push(path.clone());
            }
        }
    }
    let _ = changed_key;
    tracing::debug!(
        target: SALSA_TRACE_TARGET,
        closure_size = invalidated.len(),
        "import_dependents_bfs_complete"
    );
    trace_query_with_reason("reverse_dependents", false, Some("import_dependents_bfs"));
    invalidated
}

/// Salsa memoization boundary for assembly (fingerprint of inputs; heavy `ProgramAssembly` is ephemeral).
#[salsa::tracked]
pub fn program_assembly_tracked(
    db: &dyn Db,
    project: ProjectSession,
    grammar: crate::inputs::GrammarRevision,
    entry_path: PathBuf,
    lockfile_digest: String,
    options_fingerprint: String,
) -> String {
    let _ = (db, project, grammar);
    trace_query("program_assembly_tracked", false);
    format!(
        "{}:{}:{}",
        entry_path.display(),
        lockfile_digest,
        options_fingerprint
    )
}

/// Assembled program for an entry (Salsa-backed unit materialization).
pub fn program_assembly(
    db: &mut BeskidDatabase,
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
    options: &AssemblyOptions,
) -> Result<ProgramAssembly, AssemblyError> {
    let _assembly_guard = tracing::debug_span!(
        target: SALSA_TRACE_TARGET,
        "query",
        query = "program_assembly",
        outcome = "miss",
        entry = %entry_path.display(),
    )
    .entered();

    let lockfile_digest = lockfile_digest_for_plan(plan);
    let session = db.ensure_project_session(plan, entry_path, lockfile_digest.clone());
    let grammar = db.grammar_revision();
    let options_fp = assembly_options_fingerprint(options);
    let _ = program_assembly_tracked(
        db,
        session,
        grammar,
        entry_path.to_path_buf(),
        lockfile_digest,
        options_fp,
    );

    if let Some(source) = entry_source {
        db.ensure_file_text(entry_path.to_path_buf(), source.to_string());
    }

    let db_arc = Arc::new(Mutex::new(db.clone()));
    let materializer = unit_materializer_for(db_arc, session);

    let assembly = assemble_program_with_materializer(
        plan,
        workspace,
        entry_path,
        entry_source,
        options,
        Some(materializer),
        None,
    )?;

    let _ = discovered_units(db, session, entry_path.to_path_buf());
    let _ = module_index_fingerprint(db, session, assembly.units.len() as u64);

    let grammar = db.grammar_revision();
    for unit in assembly.units.iter() {
        seed_file_from_disk(db, unit.path.clone());
        let _ = unit_imports(db, session, grammar, unit.path.clone());
    }

    Ok(assembly)
}

fn assembly_options_fingerprint(options: &AssemblyOptions) -> String {
    format!(
        "discovery={:?}:skip_parse={}:max_units={:?}",
        options.discovery,
        options.skip_parse_errors,
        options.max_units,
    )
}

fn lockfile_digest_for_plan(plan: &CompilePlan) -> String {
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
