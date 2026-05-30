//! Graph-level queries: discovery, module index, program assembly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use beskid_analysis::projects::assembly::ProgramAssembly;
use beskid_analysis::projects::model::AssemblyOptions;
use beskid_analysis::projects::{assemble_program_with_materializer, AssemblyError};
use beskid_analysis::projects::{CompilePlan, PreparedProjectWorkspace};

use crate::db::{BeskidDatabase, Db};
use crate::inputs::ProjectSession;
use crate::materializer::unit_materializer_for;
use crate::stats::record_query_miss;
use crate::unit::{seed_file_from_disk, unit_imports};

/// Discovered unit paths for an entry (query boundary marker).
pub fn discovered_units(_db: &dyn Db, _project: ProjectSession, _entry_path: PathBuf) -> Vec<String> {
    record_query_miss();
    Vec::new()
}

/// Module index fingerprint for a unit set (query boundary marker).
pub fn module_index_fingerprint(_db: &dyn Db, _project: ProjectSession, unit_count: u64) -> String {
    record_query_miss();
    format!("modules:{unit_count}")
}

/// BFS invalidation helper: units that depend on `changed` via import edges.
pub fn reverse_dependents(
    db: &dyn Db,
    project: ProjectSession,
    changed_path: PathBuf,
    candidate_paths: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let changed_key = changed_path.display().to_string();
    let mut invalidated = vec![changed_path.clone()];
    let mut queue = vec![changed_path];
    while let Some(current) = queue.pop() {
        let current_key = current.display().to_string();
        for path in &candidate_paths {
            let imports = unit_imports(db, project, path.clone());
            if imports.iter().any(|dep| dep == &current_key || dep.ends_with(&current_key))
                && !invalidated.iter().any(|p| p == path)
            {
                invalidated.push(path.clone());
                queue.push(path.clone());
            }
        }
    }
    let _ = changed_key;
    record_query_miss();
    invalidated
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
    let lockfile_digest = lockfile_digest_for_plan(plan);
    let session = db.ensure_project_session(plan, entry_path, lockfile_digest);

    if let Some(source) = entry_source {
        db.set_file_text(entry_path.to_path_buf(), source.to_string());
    }

    let db_arc = Arc::new(Mutex::new(db.clone()));
    let materializer = unit_materializer_for(Arc::clone(&db_arc), session);

    let assembly = assemble_program_with_materializer(
        plan,
        workspace,
        entry_path,
        entry_source,
        options,
        Some(materializer),
    )?;

    let _ = discovered_units(db, session, entry_path.to_path_buf());
    let _ = module_index_fingerprint(db, session, assembly.units.len() as u64);

    for unit in assembly.units.iter() {
        seed_file_from_disk(db, unit.path.clone());
        let _ = unit_imports(db, session, unit.path.clone());
    }

    Ok(assembly)
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

pub fn assemble_program_query(
    db: &mut BeskidDatabase,
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
    options: &AssemblyOptions,
) -> Result<ProgramAssembly, AssemblyError> {
    program_assembly(db, plan, workspace, entry_path, entry_source, options)
}
