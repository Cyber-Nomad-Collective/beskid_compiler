//! Extract template trees from registry `.bpk` artifacts and validate `packageKind`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use zip::ZipArchive;

use crate::error::{TemplateError, TemplateResult};
use crate::manifest::{TEMPLATE_MANIFEST_REL, load_manifest_from_template_root};

#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(rename = "packageKind", default)]
    package_kind: Option<String>,
}

pub fn extract_bpk_to_dir(bytes: &[u8], dest: &Path) -> TemplateResult<PathBuf> {
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;

    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| TemplateError::Internal(format!("invalid .bpk zip: {e}")))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| TemplateError::Internal(e.to_string()))?;
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

    verify_template_package(dest)?;
    load_manifest_from_template_root(dest)?;
    Ok(dest.to_path_buf())
}

pub fn verify_template_package(root: &Path) -> TemplateResult<()> {
    let package_json = root.join("package.json");
    if package_json.is_file() {
        let text = fs::read_to_string(&package_json)?;
        let parsed: PackageJson = serde_json::from_str(&text)?;
        if parsed.package_kind.as_deref() != Some("template") {
            return Err(TemplateError::NotTemplatePackage { package_id: root.display().to_string() });
        }
    }
    if !root.join(TEMPLATE_MANIFEST_REL).is_file() {
        return Err(TemplateError::InvalidManifest(format!("missing {}", TEMPLATE_MANIFEST_REL)));
    }
    Ok(())
}
