use std::collections::BTreeMap;
use std::io::Cursor;

use serde_json::Value;
use zip::ZipArchive;

use crate::errors::ArtifactError;
use crate::model::{ArtifactDocumentation, ArtifactFile, ArtifactFileContent, BrowseEntry, ValidatedArtifact};
use crate::validate::validate_package_artifact;
use crate::zip_support::{
    MAX_API_DOCS_BYTES, MAX_BROWSE_READ_BYTES, normalize_zip_path, read_entry_limited, sha256_hex,
};

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
            // A hidden segment inside the public documentation or source trees
            // makes the whole artifact unbrowseable (fail closed); hidden
            // entries elsewhere are merely never listed or served.
            if (is_documentation_path(&path) || is_source_path(&path)) && !is_public_browse_entry(&path) {
                return Err(ArtifactError::UnsafeBrowseEntry(path));
            }
            entries.insert(path.clone(), BrowseEntry { path: path.clone(), size_bytes: entry.size() });
            indices.insert(path, index);
        }
        Ok(Self { bytes: bytes.to_vec(), entries, indices })
    }

    pub fn list_docs(&self) -> Result<Vec<BrowseEntry>, ArtifactError> {
        let mut entries = self
            .entries
            .values()
            .filter(|entry| is_documentation_path(&entry.path) && is_visible_path(&entry.path))
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            documentation_sort_rank(&left.path)
                .cmp(&documentation_sort_rank(&right.path))
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(entries)
    }

    pub fn read_doc(&self, path: &str) -> Result<String, ArtifactError> {
        self.read_text(path, is_documentation_path, MAX_BROWSE_READ_BYTES)
    }

    pub fn list_source_tree(&self) -> Result<Vec<BrowseEntry>, ArtifactError> {
        Ok(self
            .entries
            .values()
            .filter(|entry| is_source_path(&entry.path) && is_visible_path(&entry.path))
            .cloned()
            .collect())
    }

    pub fn read_source(&self, path: &str) -> Result<String, ArtifactError> {
        self.read_text(path, is_source_path, MAX_BROWSE_READ_BYTES)
    }

    /// Every file of the verified artifact that is safe to show in a package
    /// UI, in archive path order. Hidden entries (a dot-prefixed path segment
    /// outside `.beskid/docs/`) are never listed and never readable.
    pub fn list_files(&self) -> Vec<BrowseEntry> {
        self.entries.values().filter(|entry| is_visible_path(&entry.path)).cloned().collect()
    }

    /// Reads one listed file. Text is returned decoded; binary or oversized
    /// entries are described without their bytes so a UI can guard rendering.
    pub fn read_file(&self, requested_path: &str) -> Result<ArtifactFile, ArtifactError> {
        let path = normalize_browse_request(requested_path)?;
        if !is_visible_path(&path) {
            return Err(ArtifactError::ForbiddenBrowsePath);
        }
        let entry = self.entries.get(&path).ok_or(ArtifactError::EntryNotFound)?;
        if entry.size_bytes > MAX_BROWSE_READ_BYTES {
            return Ok(ArtifactFile {
                path,
                size_bytes: entry.size_bytes,
                content: ArtifactFileContent::TooLarge { limit_bytes: MAX_BROWSE_READ_BYTES },
            });
        }
        let mut zip = ZipArchive::new(Cursor::new(self.bytes.as_slice()))
            .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        let index = *self.indices.get(&entry.path).ok_or(ArtifactError::EntryNotFound)?;
        let bytes = read_entry_limited(&mut zip, index, &entry.path, MAX_BROWSE_READ_BYTES)?;
        let content = if looks_binary(&bytes) {
            ArtifactFileContent::Binary
        } else {
            match String::from_utf8(bytes) {
                Ok(text) => ArtifactFileContent::Text(text),
                Err(_) => ArtifactFileContent::Binary,
            }
        };
        Ok(ArtifactFile { path, size_bytes: entry.size_bytes, content })
    }

    /// Public documentation: the README plus the generated `api.json` the
    /// `beskid pckg pack` library profile embeds under `.beskid/docs/`.
    pub fn documentation(&self) -> Result<ArtifactDocumentation, ArtifactError> {
        let readme = if self.entries.contains_key("README.md") { Some(self.read_doc("README.md")?) } else { None };
        let api = if self.entries.contains_key(API_JSON_PATH) {
            let contents = self.read_text(API_JSON_PATH, is_documentation_path, MAX_API_DOCS_BYTES)?;
            let value: Value = serde_json::from_str(&contents)
                .map_err(|error| ArtifactError::InvalidDocumentation(format!("api.json is not JSON: {error}")))?;
            let Some(object) = value.as_object() else {
                return Err(ArtifactError::InvalidDocumentation("api.json root must be an object".into()));
            };
            if !object.get("schemaVersion").is_some_and(Value::is_u64)
                || !object.get("items").is_some_and(Value::is_array)
            {
                return Err(ArtifactError::InvalidDocumentation(
                    "api.json must carry a numeric schemaVersion and an items array".into(),
                ));
            }
            Some(value)
        } else {
            None
        };
        Ok(ArtifactDocumentation { readme, api })
    }

    fn read_text(
        &self,
        requested_path: &str,
        allowed: fn(&str) -> bool,
        limit_bytes: u64,
    ) -> Result<String, ArtifactError> {
        let path = normalize_browse_request(requested_path)?;
        if !allowed(&path) || !is_visible_path(&path) {
            return Err(ArtifactError::ForbiddenBrowsePath);
        }
        let entry = self.entries.get(&path).ok_or(ArtifactError::EntryNotFound)?;
        if entry.size_bytes > limit_bytes {
            return Err(ArtifactError::EntryTooLarge { path, limit_bytes });
        }
        let mut zip = ZipArchive::new(Cursor::new(self.bytes.as_slice()))
            .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        let index = *self.indices.get(&entry.path).ok_or(ArtifactError::EntryNotFound)?;
        let bytes = read_entry_limited(&mut zip, index, &entry.path, limit_bytes)?;
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
    path.starts_with("src/")
        || path.starts_with("content/")
        || path.starts_with("workspace/")
        || path.starts_with("item/")
}

fn is_public_browse_entry(path: &str) -> bool {
    !path
        .split('/')
        .enumerate()
        .any(|(index, segment)| segment.starts_with('.') && !(index == 0 && segment == ".beskid"))
}

const API_JSON_PATH: &str = ".beskid/docs/api.json";

/// Hidden segments are never exposed; the generated `.beskid/docs/` tree is
/// the one sanctioned exception because it is the package's public API model.
fn is_visible_path(path: &str) -> bool {
    if let Some(generated) = path.strip_prefix(".beskid/docs/") {
        return !generated.split('/').any(|segment| segment.starts_with('.'));
    }
    !path.split('/').any(|segment| segment.starts_with('.'))
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
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
