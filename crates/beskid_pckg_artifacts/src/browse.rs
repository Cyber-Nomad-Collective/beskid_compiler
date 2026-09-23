use std::collections::BTreeMap;
use std::io::Cursor;

use serde_json::Value;
use zip::ZipArchive;

use crate::errors::ArtifactError;
use crate::model::{ArtifactDocumentation, BrowseEntry, ValidatedArtifact};
use crate::validate::validate_package_artifact;
use crate::zip_support::{MAX_BROWSE_READ_BYTES, normalize_zip_path, read_entry_limited, sha256_hex};

/// Read-only index over a fully validated `.bpk` archive.
///
/// Construction repeats package validation using the recorded package identity
/// and checksum.  This keeps server handlers from accidentally browsing bytes
/// that merely *claim* to be a validated artifact.
#[derive(Debug, Clone)]
pub struct ArtifactBrowser {
    bytes: Vec<u8>,
    entries: BTreeMap<String, BrowseEntry>,
    indices: BTreeMap<String, usize>,
}

impl ArtifactBrowser {
    pub fn from_validated_bytes(bytes: &[u8], validated: &ValidatedArtifact) -> Result<Self, ArtifactError> {
        if sha256_hex(bytes) != validated.checksum_sha256 {
            return Err(ArtifactError::ChecksumMismatch);
        }
        validate_package_artifact(bytes, &validated.package_name, &validated.version)?;

        let mut zip =
            ZipArchive::new(Cursor::new(bytes)).map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        let mut entries = BTreeMap::new();
        let mut indices = BTreeMap::new();
        for index in 0..zip.len() {
            let entry = zip.by_index(index).map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let path = normalize_zip_path(entry.name())?;
            if !is_browseable_archive_entry(&path) {
                return Err(ArtifactError::UnsafeBrowseEntry(path));
            }
            entries.insert(path.clone(), BrowseEntry { path: path.clone(), size_bytes: entry.size() });
            indices.insert(path, index);
        }
        Ok(Self { bytes: bytes.to_vec(), entries, indices })
    }

    pub fn list_docs(&self) -> Result<Vec<BrowseEntry>, ArtifactError> {
        let mut entries =
            self.entries.values().filter(|entry| is_documentation_path(&entry.path)).cloned().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            documentation_sort_rank(&left.path)
                .cmp(&documentation_sort_rank(&right.path))
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(entries)
    }

    pub fn read_doc(&self, path: &str) -> Result<String, ArtifactError> {
        self.read_text(path, is_documentation_path)
    }

    pub fn list_source_tree(&self) -> Result<Vec<BrowseEntry>, ArtifactError> {
        Ok(self.entries.values().filter(|entry| is_source_path(&entry.path)).cloned().collect())
    }

    pub fn read_source(&self, path: &str) -> Result<String, ArtifactError> {
        self.read_text(path, is_source_path)
    }

    pub fn documentation(&self) -> Result<ArtifactDocumentation, ArtifactError> {
        let readme = if self.entries.contains_key("README.md") { Some(self.read_doc("README.md")?) } else { None };
        let metadata = if self.entries.contains_key(".beskid/docs/metadata.json") {
            let contents = self.read_doc(".beskid/docs/metadata.json")?;
            let value: Value = serde_json::from_str(&contents)
                .map_err(|error| ArtifactError::InvalidDocumentation(format!("metadata.json is not JSON: {error}")))?;
            if !value.is_object() {
                return Err(ArtifactError::InvalidDocumentation("metadata.json root must be an object".into()));
            }
            Some(value)
        } else {
            None
        };
        Ok(ArtifactDocumentation { readme, metadata })
    }

    fn read_text(&self, requested_path: &str, allowed: fn(&str) -> bool) -> Result<String, ArtifactError> {
        let path = normalize_browse_request(requested_path)?;
        if !allowed(&path) {
            return Err(ArtifactError::ForbiddenBrowsePath);
        }
        let entry = self.entries.get(&path).ok_or(ArtifactError::EntryNotFound)?;
        if entry.size_bytes > MAX_BROWSE_READ_BYTES {
            return Err(ArtifactError::EntryTooLarge { path, limit_bytes: MAX_BROWSE_READ_BYTES });
        }
        let mut zip = ZipArchive::new(Cursor::new(self.bytes.as_slice()))
            .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        let index = *self.indices.get(&entry.path).ok_or(ArtifactError::EntryNotFound)?;
        let bytes = read_entry_limited(&mut zip, index, &entry.path)?;
        String::from_utf8(bytes).map_err(|_| ArtifactError::InvalidZip("text entry is not UTF-8".into()))
    }
}

fn normalize_browse_request(path: &str) -> Result<String, ArtifactError> {
    normalize_zip_path(path).map_err(|_| ArtifactError::ForbiddenBrowsePath)
}

fn is_documentation_path(path: &str) -> bool {
    path == "README.md" || path.starts_with("docs/") || path.starts_with(".beskid/docs/")
}

fn is_source_path(path: &str) -> bool {
    path.starts_with("src/") || path.starts_with("content/") || path.starts_with("workspace/") || path.starts_with("item/")
}

fn is_browseable_archive_entry(path: &str) -> bool {
    if matches!(path, "package.json" | "template.json" | "checksums.sha256")
        || (!path.contains('/') && path.ends_with(".bproj"))
    {
        return true;
    }
    if is_documentation_path(path) || is_source_path(path) {
        return !path
            .split('/')
            .any(|segment| segment.starts_with('.') && !(segment == ".beskid" && path.starts_with(".beskid/")));
    }
    false
}

fn documentation_sort_rank(path: &str) -> u8 {
    if path == "README.md" {
        0
    } else if path.starts_with(".beskid/docs/") {
        1
    } else {
        2
    }
}
