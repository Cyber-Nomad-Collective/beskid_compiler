//! Per-unit Salsa queries: parse, expand, HIR, imports.

use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::assembly::{
    SourceUnit, UnitHir, build_hir_units, loader::expand_syntax_for_assembly,
};
use beskid_analysis::services::parse_program_with_source_name;

use crate::db::Db;
use crate::inputs::{FileText, ProjectSession};
use crate::stats::{record_query_hit, record_query_miss};

/// Parse and macro-expand one compilation unit.
#[salsa::tracked]
pub fn parse_and_expand_unit(
    db: &dyn Db,
    project: ProjectSession,
    path: PathBuf,
) -> Arc<SourceUnit> {
    let _ = project;
    record_query_miss();
    let text = resolve_unit_text(db, &path);
    let logical_name = path.display().to_string();
    let program = parse_program_with_source_name(&logical_name, &text)
        .map(expand_syntax_for_assembly)
        .expect("unit must parse");
    Arc::new(SourceUnit {
        logical_name,
        path: path.clone(),
        source: text,
        program,
    })
}

/// HIR for one unit (depends on parse query).
#[salsa::tracked]
pub fn unit_hir(db: &dyn Db, project: ProjectSession, path: PathBuf) -> Arc<UnitHir> {
    let unit = parse_and_expand_unit(db, project, path);
    record_query_miss();
    build_hir_units(&[(*unit).clone()])
        .into_iter()
        .next()
        .map(|hir| Arc::new(hir))
        .expect("unit hir")
}

/// Import paths declared in a unit source file.
#[salsa::tracked]
pub fn unit_imports(db: &dyn Db, project: ProjectSession, path: PathBuf) -> Arc<Vec<String>> {
    let unit = parse_and_expand_unit(db, project, path);
    record_query_hit();
    Arc::new(beskid_analysis::projects::assembly::unit_cache::import_paths_from_source(
        &unit.source,
    ))
}

fn resolve_unit_text(db: &dyn Db, path: &std::path::Path) -> String {
    if let Some(file) = db.file_registry().lock().expect("file registry").get(
        &path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
    ) {
        record_query_hit();
        return file.text(db).clone();
    }
    std::fs::read_to_string(path).unwrap_or_else(|_| String::new())
}

/// Seed a file input from disk or persistence before querying.
pub fn seed_file_from_disk(db: &mut crate::db::BeskidDatabase, path: PathBuf) -> FileText {
    let canonical = path.canonicalize().unwrap_or(path.clone());
    if db.file_text(&canonical).is_some() {
        return db.file_text(&canonical).expect("seeded");
    }
    let text = db
        .persistence_root()
        .and_then(|root| crate::persistence::load_persisted_file_text(root, &canonical))
        .or_else(|| std::fs::read_to_string(&canonical).ok())
        .unwrap_or_default();
    db.set_file_text(canonical.clone(), text);
    db.file_text(&canonical).expect("seeded after set")
}
