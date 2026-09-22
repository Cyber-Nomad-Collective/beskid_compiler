//! Per-unit Salsa queries: content fingerprints + cached artifacts.

use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::assembly::SourceUnit;
use beskid_analysis::services::parse_program_with_source_name;

use crate::db::{BeskidDatabase, Db};
use crate::inputs::{GrammarRevision, ProjectSession};
use crate::stats::{trace_query, trace_query_with_reason};

fn expand_syntax_for_assembly(
    program: beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program> {
    crate::expand::expand_syntax_for_assembly(program)
}

/// Content fingerprint for a unit; invalidates when file text changes.
pub fn unit_content_fingerprint(path: &std::path::Path, source: &str) -> String {
    fingerprint(path, source)
}

fn unit_source_fingerprint(db: &dyn Db, path: &std::path::Path) -> String {
    let text = resolve_unit_text(db, path);
    fingerprint(path, &text)
}

/// Import paths for a unit (tracked via file text + grammar).
#[salsa::tracked]
pub fn unit_imports(db: &dyn Db, project: ProjectSession, grammar: GrammarRevision, path: PathBuf) -> Vec<String> {
    let _ = (project, grammar);
    let display_path = path.display().to_string();
    let text = resolve_unit_text(db, &path);
    trace_query("unit_imports", true);
    let imports = import_paths_from_source(&text);
    log::info!(
        target: "beskid_queries::unit",
        "unit_imports path={} imports=[{}]",
        display_path,
        imports.join(", ")
    );
    imports
}

/// Salsa memoization token for parse+expand (heavy `SourceUnit` lives in `unit_cache`).
#[salsa::tracked]
pub fn parse_and_expand_unit_tracked(
    db: &dyn Db,
    project: ProjectSession,
    grammar: GrammarRevision,
    path: PathBuf,
    content_fp: String,
) -> String {
    let _ = (project, grammar);
    trace_query("parse_and_expand_unit_tracked", false);
    materialize_parsed_unit(db, &path, &content_fp);
    content_fp
}

/// Parsed source unit (public facade over tracked query + unit cache).
pub fn parse_and_expand_unit(db: &dyn Db, project: ProjectSession, path: PathBuf) -> SourceUnit {
    let grammar = grammar_for(db);
    let content_fp = unit_source_fingerprint(db, &path);
    let cache_hit = db.unit_cache().lock().expect("unit cache").source_units.contains_key(&content_fp);
    trace_query("parse_and_expand_unit", cache_hit);
    let _ = parse_and_expand_unit_tracked(db, project, grammar, path.clone(), content_fp.clone());
    let cache = db.unit_cache().lock().expect("unit cache");
    let unit = cache.source_units.get(&content_fp).expect("parsed unit");
    let logical_name = path.display().to_string();
    SourceUnit::bind_request(path, logical_name, unit.source.clone(), unit.program.clone())
}

/// Parsed source unit using caller-provided source (parallel-safe; no file registry write).
pub fn parse_and_expand_unit_with_source(
    db: &dyn Db,
    _project: ProjectSession,
    path: PathBuf,
    text: &str,
) -> SourceUnit {
    let content_fp = fingerprint(&path, text);
    if let Some(cached) = db.unit_cache().lock().expect("unit cache").source_units.get(&content_fp) {
        trace_query("parse_and_expand_unit_with_source", true);
        let logical_name = path.display().to_string();
        return SourceUnit::bind_request(path, logical_name, text.to_string(), cached.program.clone());
    }
    trace_query("parse_and_expand_unit_with_source", false);
    materialize_parsed_unit_from_text(db, &path, text, &content_fp)
}

fn materialize_parsed_unit(db: &dyn Db, path: &std::path::Path, content_fp: &str) {
    if db.unit_cache().lock().expect("unit cache").source_units.contains_key(content_fp) {
        return;
    }
    let text = resolve_unit_text(db, path);
    materialize_parsed_unit_from_text(db, path, &text, content_fp);
}

fn materialize_parsed_unit_from_text(db: &dyn Db, path: &std::path::Path, text: &str, content_fp: &str) -> SourceUnit {
    let logical_name = path.display().to_string();
    let program =
        parse_program_with_source_name(&logical_name, text).map(expand_syntax_for_assembly).expect("unit must parse");
    let unit = SourceUnit::bind_request(path.to_path_buf(), logical_name, text.to_string(), program);
    db.unit_cache().lock().expect("unit cache").source_units.insert(content_fp.to_string(), Arc::new(unit.clone()));
    unit
}

fn grammar_for(db: &dyn Db) -> GrammarRevision {
    db.grammar_revision_input()
}

fn resolve_unit_text(db: &dyn Db, path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(file) = db.file_registry().lock().expect("file registry").get(&canonical) {
        trace_query_with_reason("resolve_unit_text", true, Some("file_registry"));
        return file.text(db).clone();
    }
    trace_query_with_reason("resolve_unit_text", false, Some("disk_read"));
    std::fs::read_to_string(path).unwrap_or_else(|_| String::new())
}

fn fingerprint(_path: &std::path::Path, source: &str) -> String {
    beskid_artifacts::content_fingerprint(source)
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
            let import_path =
                without_comment.split_once(" as ").map(|(path, _)| path.trim()).unwrap_or(without_comment);
            if !import_path.is_empty() {
                paths.push(import_path.to_string());
            }
        }
    }
    paths
}

/// Seed a file input from disk or persistence before querying.
pub fn seed_file_from_disk(db: &mut BeskidDatabase, path: PathBuf) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_public_query_rebinds_content_cache(supplied_source: bool) {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("Real.bd");
        let copy = root.path().join("Copy.bd");
        let source = "i32 Main() { return 0; }";
        std::fs::write(&real, source).unwrap();
        std::fs::write(&copy, source).unwrap();
        let real = real.canonicalize().unwrap();
        let copy = copy.canonicalize().unwrap();
        for paths in [[&real, &copy, &real], [&copy, &real, &copy]] {
            let db = BeskidDatabase::default();
            let session = ProjectSession::new(&db, root.path().into(), real.clone(), "App".into(), "lock".into());
            for path in paths {
                let unit = if supplied_source {
                    parse_and_expand_unit_with_source(&db, session, path.clone(), source)
                } else {
                    parse_and_expand_unit(&db, session, path.clone())
                };
                assert_eq!(&unit.origin_path, path, "supplied_source={supplied_source}");
                assert_eq!(&unit.path, path);
                assert_eq!(unit.logical_name, path.display().to_string());
                assert_eq!(db.unit_cache().lock().unwrap().source_units.len(), 1);
            }
        }
    }

    #[test]
    fn public_query_rebinds_content_cache_origin_in_both_request_orders() {
        assert_public_query_rebinds_content_cache(false);
    }

    #[test]
    fn supplied_source_query_rebinds_content_cache_origin_in_both_request_orders() {
        assert_public_query_rebinds_content_cache(true);
    }
}
