//! On-disk unit artifact cache under `obj/beskid/cache/units/`.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::loader::expand_syntax_for_assembly;
use super::{SourceUnit, UnitHir, build_hir_units};

/// Grammar revision baked into unit fingerprints (bump when parse/HIR semantics change).
pub const GRAMMAR_REV: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitCacheManifest {
    pub grammar_rev: String,
    pub compiler_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedUnitRecord {
    pub fingerprint: String,
    pub logical_name: String,
    pub path: PathBuf,
    pub source: String,
    pub imports: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnitCacheStats {
    pub hits: usize,
    pub misses: usize,
}

/// Resolve cache root for a project: `{project_root}/obj/beskid/cache`.
pub fn cache_root_for_project(project_root: &Path) -> PathBuf {
    project_root.join("obj").join("beskid").join("cache")
}

pub fn unit_fingerprint(path: &Path, source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    source.hash(&mut hasher);
    GRAMMAR_REV.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn ensure_manifest(project_root: &Path) -> std::io::Result<()> {
    let root = cache_root_for_project(project_root);
    fs::create_dir_all(root.join("units"))?;
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        let manifest = UnitCacheManifest {
            grammar_rev: GRAMMAR_REV.to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        fs::write(manifest_path, serde_json::to_string_pretty(&manifest).unwrap())?;
    }
    Ok(())
}

pub fn read_cached_unit(fingerprint: &str, project_root: &Path) -> Option<CachedUnitRecord> {
    let record_path = cache_root_for_project(project_root)
        .join("units")
        .join(fingerprint)
        .join("record.json");
    let text = fs::read_to_string(record_path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_cached_unit(project_root: &Path, record: &CachedUnitRecord) -> std::io::Result<()> {
    let unit_dir = cache_root_for_project(project_root)
        .join("units")
        .join(&record.fingerprint);
    fs::create_dir_all(&unit_dir)?;
    fs::write(
        unit_dir.join("record.json"),
        serde_json::to_string_pretty(record).unwrap(),
    )?;
    fs::write(
        unit_dir.join("imports.json"),
        serde_json::to_string(&record.imports).unwrap(),
    )?;
    Ok(())
}

pub fn source_unit_from_record(record: &CachedUnitRecord) -> SourceUnit {
    let program = crate::services::parse_program_with_source_name(&record.logical_name, &record.source)
        .map(expand_syntax_for_assembly)
        .expect("cached unit must parse");
    SourceUnit {
        logical_name: record.logical_name.clone(),
        path: record.path.clone(),
        source: record.source.clone(),
        program,
    }
}

pub fn hir_from_cached_record(record: &CachedUnitRecord) -> UnitHir {
    let unit = source_unit_from_record(record);
    build_hir_units(std::slice::from_ref(&unit))
        .into_iter()
        .next()
        .expect("cached unit hir")
}

pub fn import_paths_from_source(source: &str) -> Vec<String> {
    super::loader::import_paths_from_source_full(source)
}
