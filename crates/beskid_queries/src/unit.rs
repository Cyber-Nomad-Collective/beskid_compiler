//! Per-unit Salsa queries: content fingerprints + cached artifacts.

use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::assembly::{
    ModuleIndex, ProgramAssembly, SourceUnit, UnitHir, build_hir_units,
};
use beskid_analysis::resolve::Resolver;
use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::types::build_unit_type_surface;

use crate::db::{BeskidDatabase, Db};
use crate::inputs::{GrammarRevision, ProjectSession};
use crate::output::{SharedUnitResolution, SharedUnitTypeSurface};
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
pub fn unit_imports(
    db: &dyn Db,
    project: ProjectSession,
    grammar: GrammarRevision,
    path: PathBuf,
) -> Vec<String> {
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

/// Salsa memoization token for HIR (heavy `UnitHir` lives in `unit_cache`).
#[salsa::tracked]
pub fn unit_hir_tracked(
    db: &dyn Db,
    project: ProjectSession,
    grammar: GrammarRevision,
    path: PathBuf,
    content_fp: String,
) -> String {
    let _ = grammar;
    trace_query("unit_hir_tracked", false);
    let _ = parse_and_expand_unit_tracked(db, project, grammar, path.clone(), content_fp.clone());
    materialize_unit_hir(db, &path, &content_fp);
    content_fp
}

/// Salsa memoization token for per-unit resolution (heavy [`Resolution`] lives in `unit_cache`).
#[salsa::tracked]
pub fn unit_resolution_tracked(
    db: &dyn Db,
    project: ProjectSession,
    grammar: GrammarRevision,
    path: PathBuf,
    content_fp: String,
    module_index_fp: String,
) -> String {
    let _ = grammar;
    trace_query("unit_resolution_tracked", false);
    let _ = unit_hir_tracked(db, project, grammar, path.clone(), content_fp.clone());
    materialize_unit_resolution(db, &path, &content_fp, &module_index_fp);
    content_fp
}

/// Salsa memoization token for per-unit type surface (heavy [`UnitTypeSurface`] lives in `unit_cache`).
#[salsa::tracked]
pub fn unit_type_surface_tracked(
    db: &dyn Db,
    project: ProjectSession,
    grammar: GrammarRevision,
    path: PathBuf,
    content_fp: String,
    module_index_fp: String,
) -> String {
    let _ = grammar;
    trace_query("unit_type_surface_tracked", false);
    let _ = unit_resolution_tracked(
        db,
        project,
        grammar,
        path.clone(),
        content_fp.clone(),
        module_index_fp,
    );
    materialize_unit_type_surface(db, &path, &content_fp);
    content_fp
}

/// Fingerprint for a cached [`ModuleIndex`] snapshot (assembly-scoped).
pub fn module_index_fingerprint_for_assembly(assembly: &ProgramAssembly) -> String {
    format!(
        "mi:{}:{}:{}",
        assembly.units.len(),
        assembly.module_index.prefetched_paths().len(),
        assembly.entry_index
    )
}

/// Register assembly module index for per-unit resolution/surface queries.
pub fn cache_module_index_for_assembly(db: &BeskidDatabase, assembly: &ProgramAssembly) {
    let fingerprint = module_index_fingerprint_for_assembly(assembly);
    db.cache_module_index(fingerprint, Arc::clone(&assembly.module_index));
}

/// Resolve one unit against the cached module index (or standalone resolve when absent).
pub fn unit_resolution(
    db: &dyn Db,
    project: ProjectSession,
    path: PathBuf,
    module_index_fp: &str,
) -> SharedUnitResolution {
    let grammar = grammar_for(db);
    let content_fp = unit_source_fingerprint(db, &path);
    if let Some(resolution) = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_resolutions
        .get(&content_fp)
    {
        trace_query("unit_resolution", true);
        return Arc::clone(resolution).into();
    }
    trace_query("unit_resolution", false);
    let _ = unit_resolution_tracked(
        db,
        project,
        grammar,
        path,
        content_fp.clone(),
        module_index_fp.to_string(),
    );
    Arc::clone(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_resolutions
            .get(&content_fp)
            .expect("unit resolution"),
    )
    .into()
}

/// Per-unit exported type surface (Salsa-backed; replaces disk re-parse prefetch typing).
pub fn unit_type_surface(
    db: &dyn Db,
    project: ProjectSession,
    path: PathBuf,
    module_index_fp: &str,
) -> SharedUnitTypeSurface {
    let grammar = grammar_for(db);
    if let Some(text) = registered_unit_text(db, &path) {
        let content_fp = fingerprint(&path, &text);
        if let Some(surface) = db
            .unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .get(&content_fp)
        {
            trace_query("unit_type_surface", true);
            return Arc::clone(surface).into();
        }
    }
    let content_fp = unit_source_fingerprint(db, &path);
    if let Some(surface) = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_type_surfaces
        .get(&content_fp)
    {
        trace_query("unit_type_surface", true);
        return Arc::clone(surface).into();
    }
    trace_query("unit_type_surface", false);
    let _ = unit_type_surface_tracked(
        db,
        project,
        grammar,
        path,
        content_fp.clone(),
        module_index_fp.to_string(),
    );
    Arc::clone(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .get(&content_fp)
            .expect("unit type surface"),
    )
    .into()
}

/// Warm cached type surfaces for module-index prefetch paths (no disk re-parse in type check).
pub fn warm_prefetched_unit_type_surfaces(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    assembly: &ProgramAssembly,
) {
    let module_index_fp = module_index_fingerprint_for_assembly(assembly);
    cache_module_index_for_assembly(db, assembly);
    let grammar = db.grammar_revision_ref();
    for path in assembly.module_index.prefetched_paths() {
        if assembly
            .hir_units
            .iter()
            .any(|unit| beskid_analysis::paths::same_file(&unit.path, path))
        {
            continue;
        }
        seed_file_from_disk(db, path.clone());
        let content_fp = unit_source_fingerprint(db, path);
        let _ = unit_type_surface_tracked(
            db,
            project,
            grammar,
            path.clone(),
            content_fp,
            module_index_fp.clone(),
        );
    }
}

/// Parsed source unit (public facade over tracked query + unit cache).
pub fn parse_and_expand_unit(db: &dyn Db, project: ProjectSession, path: PathBuf) -> SourceUnit {
    let grammar = grammar_for(db);
    let content_fp = unit_source_fingerprint(db, &path);
    let cache_hit = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .contains_key(&content_fp);
    trace_query("parse_and_expand_unit", cache_hit);
    let _ = parse_and_expand_unit_tracked(db, project, grammar, path.clone(), content_fp.clone());
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .get(&content_fp)
        .expect("parsed unit")
        .as_ref()
        .clone()
}

/// Parsed source unit using caller-provided source (parallel-safe; no file registry write).
pub fn parse_and_expand_unit_with_source(
    db: &dyn Db,
    _project: ProjectSession,
    path: PathBuf,
    text: &str,
) -> SourceUnit {
    let content_fp = fingerprint(&path, text);
    if let Some(cached) = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .get(&content_fp)
    {
        trace_query("parse_and_expand_unit_with_source", true);
        return (**cached).clone();
    }
    trace_query("parse_and_expand_unit_with_source", false);
    materialize_parsed_unit_from_text(db, &path, text, &content_fp)
}

/// HIR for one unit.
pub fn unit_hir(db: &dyn Db, project: ProjectSession, path: PathBuf) -> Arc<UnitHir> {
    let grammar = grammar_for(db);
    if let Some(text) = registered_unit_text(db, &path) {
        let content_fp = fingerprint(&path, &text);
        if let Some(cached) = db
            .unit_cache()
            .lock()
            .expect("unit cache")
            .unit_hir
            .get(&content_fp)
        {
            trace_query("unit_hir", true);
            return Arc::clone(cached);
        }
    }
    let content_fp = unit_source_fingerprint(db, &path);
    let cache_hit = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_hir
        .contains_key(&content_fp);
    trace_query("unit_hir", cache_hit);
    let _ = unit_hir_tracked(db, project, grammar, path.clone(), content_fp.clone());
    Arc::clone(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_hir
            .get(&content_fp)
            .expect("unit hir"),
    )
}

/// HIR for one unit using caller-provided source (parallel-safe).
pub fn unit_hir_with_source(
    db: &dyn Db,
    project: ProjectSession,
    path: PathBuf,
    text: &str,
) -> Arc<UnitHir> {
    let _ = project;
    let content_fp = fingerprint(&path, text);
    if let Some(cached) = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_hir
        .get(&content_fp)
    {
        trace_query("unit_hir_with_source", true);
        return Arc::clone(cached);
    }
    trace_query("unit_hir_with_source", false);
    let unit = parse_and_expand_unit_with_source(db, project, path.clone(), text);
    let hir = build_hir_units(&[unit])
        .into_iter()
        .next()
        .expect("unit hir");
    let arc = Arc::new(hir);
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .unit_hir
        .insert(content_fp, Arc::clone(&arc));
    arc
}

fn materialize_parsed_unit(db: &dyn Db, path: &std::path::Path, content_fp: &str) {
    if db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .contains_key(content_fp)
    {
        return;
    }
    let text = resolve_unit_text(db, path);
    materialize_parsed_unit_from_text(db, path, &text, content_fp);
}

fn materialize_parsed_unit_from_text(
    db: &dyn Db,
    path: &std::path::Path,
    text: &str,
    content_fp: &str,
) -> SourceUnit {
    let logical_name = path.display().to_string();
    let program = parse_program_with_source_name(&logical_name, text)
        .map(expand_syntax_for_assembly)
        .expect("unit must parse");
    let unit = SourceUnit {
        logical_name,
        path: path.to_path_buf(),
        source: text.to_string(),
        program,
    };
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .insert(content_fp.to_string(), Arc::new(unit.clone()));
    unit
}

fn materialize_unit_hir(db: &dyn Db, path: &std::path::Path, content_fp: &str) {
    if db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_hir
        .contains_key(content_fp)
    {
        return;
    }
    let unit = db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .source_units
        .get(content_fp)
        .expect("parsed unit before hir")
        .as_ref()
        .clone();
    let hir = build_hir_units(&[unit])
        .into_iter()
        .next()
        .expect("unit hir");
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .unit_hir
        .insert(content_fp.to_string(), Arc::new(hir));
    let _ = path;
}

fn materialize_unit_resolution(
    db: &dyn Db,
    path: &std::path::Path,
    content_fp: &str,
    module_index_fp: &str,
) {
    if db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_resolutions
        .contains_key(content_fp)
    {
        return;
    }
    let module_index = module_index_for(db, module_index_fp);
    let unit_hir = {
        let cache = db.unit_cache().lock().expect("unit cache");
        Arc::clone(
            cache
                .unit_hir
                .get(content_fp)
                .expect("unit hir before resolution"),
        )
    };
    let resolution = match module_index.resolve_unit_hir(&unit_hir.hir, path) {
        Ok(resolution) => resolution,
        Err(_) => Resolver::new()
            .resolve_program(&unit_hir.hir)
            .expect("standalone unit resolution"),
    };
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .unit_resolutions
        .insert(content_fp.to_string(), Arc::new(resolution));
}

fn materialize_unit_type_surface(db: &dyn Db, path: &std::path::Path, content_fp: &str) {
    if db
        .unit_cache()
        .lock()
        .expect("unit cache")
        .unit_type_surfaces
        .contains_key(content_fp)
    {
        return;
    }
    let unit_hir = {
        let cache = db.unit_cache().lock().expect("unit cache");
        Arc::clone(
            cache
                .unit_hir
                .get(content_fp)
                .expect("unit hir before type surface"),
        )
    };
    let resolution = {
        let cache = db.unit_cache().lock().expect("unit cache");
        Arc::clone(
            cache
                .unit_resolutions
                .get(content_fp)
                .expect("unit resolution before type surface"),
        )
    };
    let surface = build_unit_type_surface(&unit_hir.hir, resolution.as_ref(), path);
    db.unit_cache()
        .lock()
        .expect("unit cache")
        .unit_type_surfaces
        .insert(content_fp.to_string(), Arc::new(surface));
}

fn module_index_for(db: &dyn Db, module_index_fp: &str) -> Arc<ModuleIndex> {
    db.module_index_cached(module_index_fp)
        .unwrap_or_else(|| Arc::new(ModuleIndex::empty()))
}

fn grammar_for(db: &dyn Db) -> GrammarRevision {
    db.grammar_revision_input()
}

fn registered_unit_text(db: &dyn Db, path: &std::path::Path) -> Option<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    db.file_registry()
        .lock()
        .expect("file registry")
        .get(&canonical)
        .map(|file| file.text(db).clone())
}

fn resolve_unit_text(db: &dyn Db, path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(file) = db
        .file_registry()
        .lock()
        .expect("file registry")
        .get(&canonical)
    {
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
            let without_comment = rest
                .split("//")
                .next()
                .unwrap_or(rest)
                .trim_end_matches(';')
                .trim();
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
