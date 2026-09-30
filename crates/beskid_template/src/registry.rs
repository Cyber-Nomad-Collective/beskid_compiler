//! Extract template trees from registry `.bpk` artifacts and validate `packageKind`.

use std::fs;
use std::path::{Component, Path, PathBuf};

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
        let safe_name =
            file.enclosed_name().ok_or_else(|| TemplateError::Internal(format!("unsafe .bpk entry: {name}")))?;
        if name.contains('\\')
            || !Path::new(&name).components().all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(TemplateError::Internal(format!("unsafe .bpk entry: {name}")));
        }
        // Registry packing moves the authoring manifest to the artifact root;
        // restore the authoring layout before validating or installing it.
        let out_path = if name == "template.json" { dest.join(TEMPLATE_MANIFEST_REL) } else { dest.join(safe_name) };
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

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    #[test]
    fn packed_template_manifest_is_restored_to_authoring_path() {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive.start_file("package.json", SimpleFileOptions::default()).unwrap();
        archive.write_all(br#"{"packageKind":"template"}"#).unwrap();
        archive.start_file("template.json", SimpleFileOptions::default()).unwrap();
        archive
            .write_all(br#"{"schema":"beskid.template.v1","identity":"beskid.templates.lib","name":"Library","shortName":"lib"}"#)
            .unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let dest = std::env::temp_dir().join(format!("beskid-template-extract-{}", uuid::Uuid::new_v4()));

        let extracted = extract_bpk_to_dir(&bytes, &dest);
        assert!(extracted.is_ok(), "packed manifest should install: {extracted:?}");
        assert!(dest.join(TEMPLATE_MANIFEST_REL).is_file());
        assert_eq!(load_manifest_from_template_root(&dest).unwrap().short_name, "lib");
        fs::remove_dir_all(dest).unwrap();
    }

    #[test]
    fn packed_template_rejects_parent_path_without_writing_outside_destination() {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive.start_file("package.json", SimpleFileOptions::default()).unwrap();
        archive.write_all(br#"{"packageKind":"template"}"#).unwrap();
        archive.start_file("template.json", SimpleFileOptions::default()).unwrap();
        archive
            .write_all(br#"{"schema":"beskid.template.v1","identity":"beskid.templates.lib","name":"Library","shortName":"lib"}"#)
            .unwrap();
        archive.start_file("../outside.txt", SimpleFileOptions::default()).unwrap();
        archive.write_all(b"must stay inside archive").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let root = std::env::temp_dir().join(format!("beskid-template-path-{}", uuid::Uuid::new_v4()));
        let dest = root.join("extracted");
        let outside = root.join("outside.txt");

        let result = extract_bpk_to_dir(&bytes, &dest);
        let escaped = outside.exists();
        fs::remove_dir_all(root).unwrap();
        assert!(result.is_err(), "parent path in a registry package must be rejected");
        assert!(!escaped, "registry package wrote outside its destination");
    }
}
