//! Template cache read and registry install for the new-project pane.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::registry::{RegistryConnectConfig, build_pckg_client, pckg_to_anyhow, pick_version, tokio_runtime};

const TEMPLATE_MANIFEST_REL: &str = ".beskid/template.json";

const SHORT_NAME_PACKAGES: &[(&str, &str)] = &[
    ("console", "beskid.templates.console"),
    ("lib", "beskid.templates.lib"),
    ("template", "beskid.templates.project"),
];

#[derive(Debug, Clone)]
pub struct InstalledTemplateView {
    pub short_name: String,
    pub name: String,
    pub package_id: Option<String>,
    pub version: Option<String>,
    pub yanked: bool,
}

#[derive(Debug, Clone)]
pub struct RegistryTemplateView {
    pub package_id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct TemplateInstallResult {
    pub short_name: String,
    pub install_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallSnapshot {
    identity: String,
    short_name: String,
    package_id: Option<String>,
    resolved_version: Option<String>,
    installed_at: String,
    source: String,
    yanked: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateManifestJson {
    identity: String,
    name: String,
    short_name: String,
    #[allow(dead_code)]
    description: Option<String>,
}

pub fn default_registry_config() -> RegistryConnectConfig {
    let url = std::env::var("BESKID_PCKG_URL").unwrap_or_else(|_| "https://pckg.beskid-lang.org".into());
    let mut config = RegistryConnectConfig::new(url);
    if let Ok(token) = std::env::var("BESKID_PCKG_TOKEN")
        && !token.trim().is_empty()
    {
        config = config.with_bearer_token(token);
    } else if let Ok(key) = std::env::var("BESKID_PCKG_API_KEY")
        && !key.trim().is_empty()
    {
        config = config.with_api_key(key);
    }
    config
}

pub fn list_installed_templates() -> Result<Vec<InstalledTemplateView>> {
    let root = installed_root();
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
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
        let snapshot: InstallSnapshot = serde_json::from_slice(&fs::read(&snapshot_path)?)?;
        let manifest = load_manifest(&path).ok();
        let short_name = snapshot.short_name.clone();
        let name = manifest.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| short_name.clone());
        rows.push(InstalledTemplateView {
            short_name,
            name,
            package_id: snapshot.package_id,
            version: snapshot.resolved_version,
            yanked: snapshot.yanked,
        });
    }
    rows.sort_by(|a, b| a.short_name.cmp(&b.short_name));
    Ok(rows)
}

pub fn list_registry_templates(config: &RegistryConnectConfig) -> Result<Vec<RegistryTemplateView>> {
    let client = build_pckg_client(config)?;
    let runtime = tokio_runtime()?;
    let packages = runtime.block_on(client.list_packages()).map_err(pckg_to_anyhow)?;
    let mut rows = Vec::new();
    for pkg in packages {
        if !pkg.name.starts_with("beskid.templates.") {
            continue;
        }
        rows.push(RegistryTemplateView { package_id: pkg.name, description: pkg.description });
    }
    rows.sort_by(|a, b| a.package_id.cmp(&b.package_id));
    Ok(rows)
}

pub fn install_registry_template(config: &RegistryConnectConfig, package_id: &str) -> Result<TemplateInstallResult> {
    let client = build_pckg_client(config)?;
    let runtime = tokio_runtime()?;
    let versions = runtime.block_on(client.list_package_versions(package_id)).map_err(pckg_to_anyhow)?;
    let chosen = pick_version(&versions, None)?;
    let bytes =
        runtime.block_on(client.download_package_version(package_id, &chosen.version)).map_err(pckg_to_anyhow)?;

    let extract_dir =
        std::env::temp_dir().join(format!("beskid-template-{}-{}", package_id.replace('.', "_"), chosen.version));
    extract_bpk_to_dir(&bytes, &extract_dir)?;
    let manifest = load_manifest(&extract_dir)?;

    let snapshot = InstallSnapshot {
        identity: manifest.identity.clone(),
        short_name: manifest.short_name.clone(),
        package_id: Some(package_id.to_string()),
        resolved_version: Some(chosen.version.clone()),
        installed_at: chrono_lite_now(),
        source: "registry".into(),
        yanked: chosen.is_yanked,
    };
    let install_dir = install_from_tree(&extract_dir, &snapshot)?;
    Ok(TemplateInstallResult { short_name: manifest.short_name.clone(), install_dir })
}

pub fn resolve_package_id(selector: &str) -> String {
    SHORT_NAME_PACKAGES
        .iter()
        .find(|(short, _)| *short == selector)
        .map(|(_, id)| (*id).to_string())
        .unwrap_or_else(|| selector.to_string())
}

fn installed_root() -> PathBuf {
    if let Ok(dir) = std::env::var("BESKID_CONFIG_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir).join("templates").join("installed");
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("beskid").join("templates").join("installed")
}

fn install_dir_for_identity(identity: &str) -> PathBuf {
    let safe: String = identity
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    installed_root().join(safe)
}

fn load_manifest(root: &Path) -> Result<TemplateManifestJson> {
    let path = root.join(TEMPLATE_MANIFEST_REL);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

fn extract_bpk_to_dir(bytes: &[u8], dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).context("invalid .bpk zip")?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let out_path = dest.join(&name);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&out_path)?;
        std::io::copy(&mut file, &mut out)?;
    }
    load_manifest(dest)?;
    Ok(())
}

fn install_from_tree(template_root: &Path, snapshot: &InstallSnapshot) -> Result<PathBuf> {
    let manifest = load_manifest(template_root)?;
    let dest = install_dir_for_identity(&manifest.identity);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    copy_tree(template_root, &dest)?;
    write_snapshot(&dest, snapshot)?;
    Ok(dest)
}

fn write_snapshot(dest: &Path, snapshot: &InstallSnapshot) -> Result<()> {
    let path = dest.join("manifest.snapshot.json");
    fs::write(path, serde_json::to_vec_pretty(snapshot)?)?;
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        if name == "manifest.snapshot.json" {
            continue;
        }
        let src = entry.path();
        let dst = to.join(name);
        if file_type.is_dir() {
            copy_tree(&src, &dst)?;
        } else if file_type.is_file() {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{secs}")
}
