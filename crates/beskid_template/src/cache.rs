//! Installed template cache and `manifest.snapshot.json` records.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{TemplateError, TemplateResult};
use crate::manifest::{load_manifest_from_template_root, TEMPLATE_MANIFEST_REL};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSnapshot {
    pub identity: String,
    pub short_name: String,
    pub package_id: Option<String>,
    pub resolved_version: Option<String>,
    pub checksum: Option<String>,
    pub installed_at: String,
    pub source: InstallSource,
    pub yanked: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InstallSource {
    Registry,
    Path,
    Git,
}

pub fn beskid_config_root() -> PathBuf {
    if let Ok(dir) = std::env::var("BESKID_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("beskid")
}

pub fn installed_root() -> PathBuf {
    beskid_config_root().join("templates").join("installed")
}

pub fn registry_index_path() -> PathBuf {
    beskid_config_root()
        .join("templates")
        .join("registry-index.json")
}

pub fn install_dir_for_identity(identity: &str) -> PathBuf {
    let safe: String = identity
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    installed_root().join(safe)
}

pub fn list_installed() -> TemplateResult<Vec<(InstallSnapshot, PathBuf)>> {
    let root = installed_root();
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let snapshot_path = path.join("manifest.snapshot.json");
        if !snapshot_path.is_file() {
            continue;
        }
        let bytes = fs::read(&snapshot_path)?;
        let snapshot: InstallSnapshot = serde_json::from_slice(&bytes)?;
        out.push((snapshot, path));
    }
    out.sort_by(|a, b| a.0.short_name.cmp(&b.0.short_name));
    Ok(out)
}

pub fn install_from_tree(
    template_root: &Path,
    snapshot: InstallSnapshot,
) -> TemplateResult<PathBuf> {
    let manifest = load_manifest_from_template_root(template_root)?;
    let dest = install_dir_for_identity(&manifest.identity);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    copy_tree(template_root, &dest)?;
    write_snapshot(&dest, &snapshot)?;
    Ok(dest)
}

pub fn uninstall_by_short_name(short_name: &str) -> TemplateResult<bool> {
    for (snap, path) in list_installed()? {
        if snap.short_name == short_name {
            fs::remove_dir_all(&path)?;
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn find_installed_by_short_name(short_name: &str) -> TemplateResult<Option<(InstallSnapshot, PathBuf)>> {
    Ok(list_installed()?
        .into_iter()
        .find(|(s, _)| s.short_name == short_name))
}

pub fn write_snapshot(dir: &Path, snapshot: &InstallSnapshot) -> TemplateResult<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join("manifest.snapshot.json");
    let bytes = serde_json::to_vec_pretty(snapshot)?;
    fs::write(path, bytes)?;
    Ok(())
}

pub fn read_template_root_from_install(dir: &Path) -> TemplateResult<PathBuf> {
    if dir.join(TEMPLATE_MANIFEST_REL).is_file() {
        return Ok(dir.to_path_buf());
    }
    Err(TemplateError::InvalidManifest(format!(
        "installed template at {} is missing {}",
        dir.display(),
        TEMPLATE_MANIFEST_REL
    )))
}

pub fn checksum_dir(root: &Path) -> TemplateResult<String> {
    let mut hasher = Sha256::new();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(root, &mut files)?;
    files.sort();
    for file in files {
        if file.ends_with("manifest.snapshot.json") {
            continue;
        }
        hasher.update(file.strip_prefix(root).unwrap_or(&file).to_string_lossy().as_bytes());
        hasher.update(&fs::read(&file)?);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> TemplateResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> TemplateResult<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let src = entry.path();
        let dst = to.join(name);
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryIndex {
    #[serde(default)]
    pub packages: std::collections::BTreeMap<String, RegistryIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndexEntry {
    pub latest_version: String,
    pub checked_at: String,
}

pub fn load_registry_index() -> RegistryIndex {
    let path = registry_index_path();
    if !path.is_file() {
        return RegistryIndex::default();
    }
    fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_registry_index(index: &RegistryIndex) -> TemplateResult<()> {
    if let Some(parent) = registry_index_path().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(registry_index_path(), serde_json::to_vec_pretty(index)?)?;
    Ok(())
}
