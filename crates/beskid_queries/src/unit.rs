//! Per-unit Salsa queries: content fingerprints + cached artifacts.

use std::path::PathBuf;

use beskid_analysis::projects::assembly::{SourceUnit, UnitHir, build_hir_units};
use beskid_analysis::services::parse_program_with_source_name;

use crate::db::Db;
use crate::inputs::ProjectSession;
use crate::stats::{record_query_hit, record_query_miss};

fn expand_syntax_for_assembly(
    program: beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program> {
    crate::expand::expand_syntax_for_assembly(program)
}

/// Content fingerprint for a unit; invalidates when file text changes.
pub fn unit_content_fingerprint(path: &std::path::Path, source: &str) -> String {
    fingerprint(path, source)
}

/// Import paths for a unit (tracked via fingerprint).
#[salsa::tracked]
pub fn unit_imports(
    db: &dyn Db,
    project: ProjectSession,
    path: PathBuf,
) -> Vec<String> {
    let _ = project;
    let text = resolve_unit_text(db, &path);
    record_query_hit();
    import_paths_from_source(&text)
}

/// Parsed source unit (memoized in db unit cache, keyed by fingerprint).
pub fn parse_and_expand_unit(
    db: &dyn Db,
    project: ProjectSession,
    path: PathBuf,
) -> SourceUnit {
    let _ = project;
    let text = resolve_unit_text(db, &path);
    let fp = fingerprint(&path, &text);
    if let Some(cached) = db.unit_cache().lock().expect("unit cache").source_units.get(&fp) {
        record_query_hit();
        return (**cached).clone();
    }
    record_query_miss();
    let logical_name = path.display().to_string();
    let program = parse_program_with_source_name(&logical_name, &text)
        .map(expand_syntax_for_assembly)
        .expect("unit must parse");
    let unit = SourceUnit {
        logical_name,
        path: path.clone(),
        source: text,
        program,
    };
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .insert(fp, std::sync::Arc::new(unit.clone()));
    unit
}

/// HIR for one unit.
pub fn unit_hir(db: &dyn Db, project: ProjectSession, path: PathBuf) -> std::sync::Arc<UnitHir> {
    let _ = project;
    let text = resolve_unit_text(db, &path);
    let fp = fingerprint(&path, &text);
    if let Some(cached) = db.unit_cache().lock().expect("unit cache").unit_hir.get(&fp) {
        record_query_hit();
        return std::sync::Arc::clone(cached);
    }
    record_query_miss();
    let unit = parse_and_expand_unit(db, project, path.clone());
    let hir = build_hir_units(&[unit])
        .into_iter()
        .next()
        .expect("unit hir");
    let arc = std::sync::Arc::new(hir);
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .unit_hir
        .insert(fp, std::sync::Arc::clone(&arc));
    arc
}

fn resolve_unit_text(db: &dyn Db, path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(file) = db.file_registry().lock().expect("file registry").get(&canonical) {
        record_query_hit();
        return file.text(db).clone();
    }
    std::fs::read_to_string(path).unwrap_or_else(|_| String::new())
}

fn fingerprint(path: &std::path::Path, source: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    source.hash(&mut hasher);
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn import_paths_from_source(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("use ") {
            let without_comment = rest.split("//").next().unwrap_or(rest).trim_end_matches(';').trim();
            let import_path = without_comment
                .split_once(" as ")
                .map(|(path, _)| path.trim())
                .unwrap_or(without_comment);
            if !import_path.is_empty() {
                paths.push(import_path.to_string());
            }
        }
    }
    paths
}

/// Seed a file input from disk or persistence before querying.
pub fn seed_file_from_disk(db: &mut crate::db::BeskidDatabase, path: PathBuf) {
    let canonical = path.canonicalize().unwrap_or(path.clone());
    if db.file_text(&canonical).is_some() {
        return;
    }
    let text = db
        .persistence_root()
        .and_then(|root| crate::persistence::load_persisted_file_text(root, &canonical))
        .or_else(|| std::fs::read_to_string(&canonical).ok())
        .unwrap_or_default();
    db.ensure_file_text(canonical, text);
}
