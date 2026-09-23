use std::collections::BTreeMap;
use std::io::Cursor;

use zip::ZipArchive;

use crate::errors::ArtifactError;
use crate::manifest::{validate_manifest, validate_template_contract};
use crate::model::ValidatedArtifact;
use crate::project_manifest::{project_field, project_root_block_identifier, validate_published_project_dependencies};
use crate::zip_support::{
    MAX_ENTRIES, MAX_UNCOMPRESSED_BYTES, REQUIRED_ENTRIES, normalize_zip_path, parse_checksums, read_entry,
    read_entry_bytes, sha256_hex,
};

/// Parses and validates the `.bpk` format before persistence.
pub fn validate_package_artifact(
    bytes: &[u8],
    expected_package_name: &str,
    expected_version: &str,
) -> Result<ValidatedArtifact, ArtifactError> {
    if bytes.is_empty() {
        return Err(ArtifactError::EmptyArtifact);
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(ArtifactError::InvalidZip("too many entries".into()));
    }
    let mut entries = BTreeMap::new();
    let mut uncompressed = 0_u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = normalize_zip_path(entry.name())?;
        uncompressed = uncompressed.saturating_add(entry.size());
        if uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(ArtifactError::InvalidZip("uncompressed size limit exceeded".into()));
        }
        if entries.insert(name.clone(), index).is_some() {
            return Err(ArtifactError::InvalidZip(format!("duplicate entry '{name}'")));
        }
    }
    for required in REQUIRED_ENTRIES {
        if !entries.contains_key(required) {
            return Err(ArtifactError::InvalidZip(format!("missing required entry '{required}'")));
        }
    }
    if entries.keys().any(|path| forbidden_path(path)) {
        return Err(ArtifactError::InvalidZip("contains forbidden .beskid entry".into()));
    }
    let manifest_json = read_entry(&mut zip, entries["package.json"])?;
    let manifest = validate_manifest(&manifest_json, expected_package_name, expected_version, &entries)?;
    let package_kind = manifest.package_kind.as_str();
    let project_manifests =
        entries.keys().filter(|path| !path.contains('/') && path.ends_with(".bproj")).cloned().collect::<Vec<_>>();
    if package_kind != "tool" && project_manifests.len() != 1 {
        return Err(ArtifactError::InvalidZip(format!(
            "package must contain exactly one root .bproj manifest, found {}",
            project_manifests.len()
        )));
    }
    if let Some(project_manifest) = project_manifests.first() {
        let project = read_entry(&mut zip, entries[project_manifest])?;
        let project_name = project_field(&project, "name")
            .ok_or_else(|| ArtifactError::InvalidManifest(format!("{project_manifest} is missing project name")))?;
        let root_block = project_root_block_identifier(&project).ok_or_else(|| {
            ArtifactError::InvalidManifest(format!("{project_manifest} is missing a canonical project root block"))
        })?;
        if root_block != project_name {
            return Err(ArtifactError::InvalidManifest(format!(
                "{project_manifest} project root block does not match project name"
            )));
        }
        if project_manifest.strip_suffix(".bproj") != Some(project_name) {
            return Err(ArtifactError::InvalidManifest(format!(
                "{project_manifest} file name does not match project name"
            )));
        }
        if package_kind == "template" && project_field(&project, "identity") != Some(expected_package_name) {
            return Err(ArtifactError::InvalidManifest(format!(
                "{project_manifest} template identity does not match package identity"
            )));
        }
        validate_published_project_dependencies(&project, &manifest.dependencies)?;
        let has_sources = entries.keys().any(|path| path.starts_with("src/"));
        let is_aggregate = project_field(&project, "type") == Some("Aggregate");
        if package_kind == "library" && !has_sources && !is_aggregate {
            return Err(ArtifactError::InvalidZip("library package is missing source under src/".into()));
        }
    }
    if package_kind == "template"
        && !entries
            .keys()
            .any(|path| path.starts_with("content/") || path.starts_with("workspace/") || path.starts_with("item/"))
    {
        return Err(ArtifactError::InvalidZip("template package is missing scaffold payload".into()));
    }
    if package_kind == "template" {
        let template_json = read_entry(&mut zip, entries["template.json"])?;
        validate_template_contract(&template_json, expected_package_name, manifest.template.as_ref())?;
    }
    let checksums = parse_checksums(&read_entry(&mut zip, entries["checksums.sha256"])?)?;
    if checksums.contains_key("checksums.sha256") {
        return Err(ArtifactError::InvalidChecksums("checksums.sha256 must not reference itself".into()));
    }
    for (path, index) in &entries {
        if path == "checksums.sha256" {
            continue;
        }
        let expected = checksums
            .get(path)
            .ok_or_else(|| ArtifactError::InvalidChecksums(format!("missing checksum for '{path}'")))?;
        let actual = sha256_hex(&read_entry_bytes(&mut zip, *index)?);
        if &actual != expected {
            return Err(ArtifactError::InvalidChecksums(format!("checksum mismatch for '{path}'")));
        }
    }
    for path in checksums.keys() {
        if !entries.contains_key(path) {
            return Err(ArtifactError::InvalidChecksums(format!("references missing entry '{path}'")));
        }
    }
    Ok(ValidatedArtifact {
        package_name: expected_package_name.to_owned(),
        version: expected_version.to_owned(),
        checksum_sha256: sha256_hex(bytes),
        size_bytes: bytes.len() as u64,
        manifest_json,
        metadata: manifest,
    })
}

pub(crate) fn forbidden_path(path: &str) -> bool {
    path.starts_with(".beskid/") && !path.starts_with(".beskid/docs/")
}
