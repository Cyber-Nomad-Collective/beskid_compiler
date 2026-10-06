//! Installed template cache and `manifest.snapshot.json` records.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{TemplateError, TemplateResult};
use crate::manifest::{TEMPLATE_MANIFEST_REL, load_manifest_from_template_root};

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
    Bundled,
    Registry,
    Path,
    Git,
}

pub fn beskid_config_root() -> PathBuf {
    if let Ok(dir) = std::env::var("BESKID_CONFIG_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("beskid")
}

pub fn installed_root() -> PathBuf {
    beskid_config_root().join("templates").join("installed")
}

pub fn registry_index_path() -> PathBuf {
    beskid_config_root().join("templates").join("registry-index.json")
}

pub fn install_dir_for_identity(identity: &str) -> PathBuf {
    let safe: String = identity
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
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

pub fn install_from_tree(template_root: &Path, mut snapshot: InstallSnapshot) -> TemplateResult<PathBuf> {
    let manifest = load_manifest_from_template_root(template_root)?;
    let inventory = TreeInventory::read(template_root)?;
    if snapshot.identity != manifest.identity || snapshot.short_name != manifest.short_name {
        return Err(TemplateError::InvalidManifest("installation snapshot identity differs from template".into()));
    }
    snapshot.checksum = Some(inventory.checksum(template_root)?);
    let dest = install_dir_for_identity(&manifest.identity);
    let parent = dest.parent().expect("installed directory parent");
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    inventory.copy(template_root, staging.path())?;
    if checksum_dir(staging.path())? != snapshot.checksum.as_deref().expect("computed checksum") {
        return Err(TemplateError::Internal("template changed during installation; retry from a stable source".into()));
    }
    write_snapshot(staging.path(), &snapshot)?;
    let backup = parent.join(format!(".backup-{}", uuid::Uuid::new_v4()));
    let had_previous = dest.exists();
    if had_previous {
        fs::rename(&dest, &backup)?;
    }
    if let Err(error) = fs::rename(staging.path(), &dest) {
        if had_previous {
            fs::rename(&backup, &dest)?;
        }
        return Err(error.into());
    }
    if had_previous {
        fs::remove_dir_all(backup)?;
    }
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
    Ok(list_installed()?.into_iter().find(|(s, _)| s.short_name == short_name))
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

pub const TEMPLATE_CHECKSUM_PREFIX: &str = "sha256-template-tree-v1:";

pub fn checksum_dir(root: &Path) -> TemplateResult<String> {
    TreeInventory::read(root)?.checksum(root)
}

struct TreeInventory {
    directories: Vec<PathBuf>,
    files: Vec<(String, PathBuf)>,
}

impl TreeInventory {
    fn read(root: &Path) -> TemplateResult<Self> {
        let metadata = fs::symlink_metadata(root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(TemplateError::InvalidManifest("template root must be a directory, not a symlink".into()));
        }
        let mut inventory = Self { directories: Vec::new(), files: Vec::new() };
        inventory.collect(root, Path::new(""))?;
        inventory.files.sort_by(|left, right| left.0.cmp(&right.0));
        inventory.directories.sort();
        Ok(inventory)
    }

    fn collect(&mut self, root: &Path, relative: &Path) -> TemplateResult<()> {
        for entry in fs::read_dir(root.join(relative))? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            let metadata = fs::symlink_metadata(root.join(&path))?;
            if metadata.file_type().is_symlink() {
                return Err(TemplateError::InvalidManifest(format!(
                    "symlink template payload is forbidden: {}",
                    path.display()
                )));
            }
            if entry.file_name() == ".git" || path == Path::new("manifest.snapshot.json") {
                continue;
            }
            if metadata.is_dir() {
                self.directories.push(path.clone());
                self.collect(root, &path)?;
            } else if metadata.is_file() {
                let normalized = path
                    .components()
                    .map(|part| {
                        part.as_os_str()
                            .to_str()
                            .ok_or_else(|| TemplateError::InvalidManifest("template paths must be UTF-8".into()))
                    })
                    .collect::<TemplateResult<Vec<_>>>()?
                    .join("/");
                self.files.push((normalized, path));
            } else {
                return Err(TemplateError::InvalidManifest(format!(
                    "non-regular template payload is forbidden: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    fn checksum(&self, root: &Path) -> TemplateResult<String> {
        let mut hasher = Sha256::new();
        hasher.update(b"beskid-template-tree-v1\0");
        for (normalized, path) in &self.files {
            let metadata = fs::symlink_metadata(root.join(path))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(TemplateError::InvalidManifest("template changed to a symlink or special file".into()));
            }
            let bytes = fs::read(root.join(path))?;
            hasher.update((normalized.len() as u64).to_le_bytes());
            hasher.update(normalized.as_bytes());
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        Ok(format!("{TEMPLATE_CHECKSUM_PREFIX}{:x}", hasher.finalize()))
    }

    fn copy(&self, root: &Path, dest: &Path) -> TemplateResult<()> {
        for directory in &self.directories {
            fs::create_dir_all(dest.join(directory))?;
        }
        for (_, path) in &self.files {
            let metadata = fs::symlink_metadata(root.join(path))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(TemplateError::InvalidManifest("template changed to a symlink or special file".into()));
            }
            fs::copy(root.join(path), dest.join(path))?;
        }
        Ok(())
    }
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
    fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_registry_index(index: &RegistryIndex) -> TemplateResult<()> {
    if let Some(parent) = registry_index_path().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(registry_index_path(), serde_json::to_vec_pretty(index)?)?;
    Ok(())
}

#[cfg(test)]
mod v06_cache_tests {
    use super::*;

    #[test]
    fn v06_cache_digest_frames_paths_and_payload_and_versions_authority() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::write(first.path().join("a"), "bc").unwrap();
        fs::write(second.path().join("ab"), "c").unwrap();
        let a = checksum_dir(first.path()).unwrap();
        let b = checksum_dir(second.path()).unwrap();
        assert_ne!(a, b, "path/payload concatenation must not collide");
        assert!(a.starts_with("sha256-template-tree-v1:"));
    }

    #[test]
    fn v06_cache_nested_snapshot_is_payload_not_install_receipt() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("nested")).unwrap();
        fs::write(root.path().join("nested/manifest.snapshot.json"), "first").unwrap();
        let before = checksum_dir(root.path()).unwrap();
        fs::write(root.path().join("nested/manifest.snapshot.json"), "second").unwrap();
        assert_ne!(before, checksum_dir(root.path()).unwrap());
        fs::write(root.path().join("manifest.snapshot.json"), "receipt").unwrap();
        let with_receipt = checksum_dir(root.path()).unwrap();
        fs::write(root.path().join("manifest.snapshot.json"), "changed receipt").unwrap();
        assert_eq!(with_receipt, checksum_dir(root.path()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn v06_cache_inventory_rejects_external_links_and_directory_cycles() {
        for cycle in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let outside = tempfile::NamedTempFile::new().unwrap();
            std::os::unix::fs::symlink(if cycle { root.path() } else { outside.path() }, root.path().join("link"))
                .unwrap();
            let error = checksum_dir(root.path()).unwrap_err().to_string();
            assert!(error.contains("symlink"), "{error}");
        }
    }
}
