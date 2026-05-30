//! On-disk persistence adapter for Salsa session state under `obj/beskid/cache/salsa/`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalsaPersistenceManifest {
    pub grammar_rev: String,
    pub compiler_version: String,
    pub persisted_files: usize,
}

pub fn ensure_salsa_dir(root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        let manifest = SalsaPersistenceManifest {
            grammar_rev: env!("CARGO_PKG_VERSION").to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            persisted_files: 0,
        };
        fs::write(manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    }
    fs::create_dir_all(root.join("files"))?;
    Ok(())
}

pub fn persist_file_text(root: &Path, path: &Path, text: &str) -> std::io::Result<()> {
    ensure_salsa_dir(root)?;
    let key = file_key(path);
    fs::write(root.join("files").join(format!("{key}.bd")), text)?;
    update_manifest_count(root)
}

pub fn load_persisted_file_text(root: &Path, path: &Path) -> Option<String> {
    let key = file_key(path);
    fs::read_to_string(root.join("files").join(format!("{key}.bd"))).ok()
}

fn file_key(path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn update_manifest_count(root: &Path) -> std::io::Result<()> {
    let count = fs::read_dir(root.join("files"))?
        .filter_map(|e| e.ok())
        .count();
    let manifest_path = root.join("manifest.json");
    let mut manifest: SalsaPersistenceManifest = if manifest_path.is_file() {
        serde_json::from_str(&fs::read_to_string(&manifest_path)?).unwrap_or(SalsaPersistenceManifest {
            grammar_rev: env!("CARGO_PKG_VERSION").to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            persisted_files: 0,
        })
    } else {
        SalsaPersistenceManifest {
            grammar_rev: env!("CARGO_PKG_VERSION").to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            persisted_files: 0,
        }
    };
    manifest.persisted_files = count;
    fs::write(manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(())
}

pub fn load_manifest(root: &Path) -> Option<SalsaPersistenceManifest> {
    let text = fs::read_to_string(root.join("manifest.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn cache_root_for_project(project_root: &Path) -> PathBuf {
    project_root.join("obj").join("beskid").join("cache").join("salsa")
}
