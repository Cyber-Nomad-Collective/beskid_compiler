//! Safe package artifact validation and local artifact storage.
//!
//! This crate owns byte-level invariants only.  Package authorization,
//! publication idempotency, and version metadata remain server/store concerns.

use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

const MAX_ENTRIES: usize = 10_000;
const MAX_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
/// Browsing is deliberately capped well below the accepted artifact size.
/// Consumers render individual files, they never need a whole source tree in
/// one response.
pub const MAX_BROWSE_READ_BYTES: u64 = 1024 * 1024;
const REQUIRED_ENTRIES: [&str; 3] = ["package.json", "Project.proj", "checksums.sha256"];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArtifactError {
    #[error("artifact is empty")]
    EmptyArtifact,
    #[error("artifact ZIP is invalid: {0}")]
    InvalidZip(String),
    #[error("artifact manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("artifact checksums are invalid: {0}")]
    InvalidChecksums(String),
    #[error("artifact storage key is invalid")]
    InvalidStorageKey,
    #[error("artifact is missing")]
    NotFound,
    #[error("artifact checksum does not match")]
    ChecksumMismatch,
    #[error("artifact I/O failed: {0}")]
    Io(String),
    #[error("artifact contains an entry that is unsafe to browse: {0}")]
    UnsafeBrowseEntry(String),
    #[error("requested artifact path is not browseable")]
    ForbiddenBrowsePath,
    #[error("artifact entry is missing")]
    EntryNotFound,
    #[error("artifact entry '{path}' exceeds the {limit_bytes} byte read limit")]
    EntryTooLarge { path: String, limit_bytes: u64 },
    #[error("structured documentation metadata is invalid: {0}")]
    InvalidDocumentation(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArtifact {
    pub package_name: String,
    pub version: String,
    pub checksum_sha256: String,
    pub size_bytes: u64,
    pub manifest_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub storage_key: String,
    pub checksum_sha256: String,
    pub size_bytes: u64,
}

/// A source or documentation entry that is safe to expose in a package UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseEntry {
    pub path: String,
    pub size_bytes: u64,
}

/// Extracted public documentation metadata.  A package may omit either field.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactDocumentation {
    pub readme: Option<String>,
    pub metadata: Option<Value>,
}

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
    pub fn from_validated_bytes(
        bytes: &[u8],
        validated: &ValidatedArtifact,
    ) -> Result<Self, ArtifactError> {
        if sha256_hex(bytes) != validated.checksum_sha256 {
            return Err(ArtifactError::ChecksumMismatch);
        }
        validate_package_artifact(bytes, &validated.package_name, &validated.version)?;

        let mut zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        let mut entries = BTreeMap::new();
        let mut indices = BTreeMap::new();
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let path = normalize_zip_path(entry.name())?;
            if !is_browseable_archive_entry(&path) {
                return Err(ArtifactError::UnsafeBrowseEntry(path));
            }
            entries.insert(
                path.clone(),
                BrowseEntry {
                    path: path.clone(),
                    size_bytes: entry.size(),
                },
            );
            indices.insert(path, index);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
            entries,
            indices,
        })
    }

    pub fn list_docs(&self) -> Result<Vec<BrowseEntry>, ArtifactError> {
        let mut entries = self
            .entries
            .values()
            .filter(|entry| is_documentation_path(&entry.path))
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
        self.read_text(path, is_documentation_path)
    }

    pub fn list_source_tree(&self) -> Result<Vec<BrowseEntry>, ArtifactError> {
        Ok(self
            .entries
            .values()
            .filter(|entry| is_source_path(&entry.path))
            .cloned()
            .collect())
    }

    pub fn read_source(&self, path: &str) -> Result<String, ArtifactError> {
        self.read_text(path, is_source_path)
    }

    pub fn documentation(&self) -> Result<ArtifactDocumentation, ArtifactError> {
        let readme = if self.entries.contains_key("README.md") {
            Some(self.read_doc("README.md")?)
        } else {
            None
        };
        let metadata = if self.entries.contains_key(".beskid/docs/metadata.json") {
            let contents = self.read_doc(".beskid/docs/metadata.json")?;
            let value: Value = serde_json::from_str(&contents).map_err(|error| {
                ArtifactError::InvalidDocumentation(format!("metadata.json is not JSON: {error}"))
            })?;
            if !value.is_object() {
                return Err(ArtifactError::InvalidDocumentation(
                    "metadata.json root must be an object".into(),
                ));
            }
            Some(value)
        } else {
            None
        };
        Ok(ArtifactDocumentation { readme, metadata })
    }

    fn read_text(
        &self,
        requested_path: &str,
        allowed: fn(&str) -> bool,
    ) -> Result<String, ArtifactError> {
        let path = normalize_browse_request(requested_path)?;
        if !allowed(&path) {
            return Err(ArtifactError::ForbiddenBrowsePath);
        }
        let entry = self
            .entries
            .get(&path)
            .ok_or(ArtifactError::EntryNotFound)?;
        if entry.size_bytes > MAX_BROWSE_READ_BYTES {
            return Err(ArtifactError::EntryTooLarge {
                path,
                limit_bytes: MAX_BROWSE_READ_BYTES,
            });
        }
        let mut zip = ZipArchive::new(Cursor::new(self.bytes.as_slice()))
            .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        let index = *self
            .indices
            .get(&entry.path)
            .ok_or(ArtifactError::EntryNotFound)?;
        let bytes = read_entry_limited(&mut zip, index, &entry.path)?;
        String::from_utf8(bytes)
            .map_err(|_| ArtifactError::InvalidZip("text entry is not UTF-8".into()))
    }
}

pub struct PublishRequest<'a> {
    pub validated: ValidatedArtifact,
    pub bytes: &'a [u8],
}

pub trait PackageArtifactStore {
    fn save(&self, request: PublishRequest<'_>) -> Result<StoredArtifact, ArtifactError>;
    fn open(&self, storage_key: &str) -> Result<Vec<u8>, ArtifactError>;
    fn verify(&self, storage_key: &str, expected_sha256: &str) -> Result<bool, ArtifactError>;
    fn delete(&self, storage_key: &str) -> Result<(), ArtifactError>;
}

/// Filesystem implementation suitable for a single-node deployment.  A future
/// object-store adapter must preserve the key and checksum semantics here.
#[derive(Debug, Clone)]
pub struct LocalFileArtifactStore {
    root: PathBuf,
}

impl LocalFileArtifactStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ArtifactError> {
        let root = fs::canonicalize(root.as_ref())
            .or_else(|_| {
                fs::create_dir_all(root.as_ref())?;
                fs::canonicalize(root.as_ref())
            })
            .map_err(io_error)?;
        Ok(Self { root })
    }

    fn path_for_key(&self, storage_key: &str) -> Result<PathBuf, ArtifactError> {
        let mut parts = storage_key.split('/');
        let (Some(package), Some(version), Some(filename), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ArtifactError::InvalidStorageKey);
        };
        if filename != "artifact.bpk"
            || !is_storage_component(package)
            || !is_storage_component(version)
        {
            return Err(ArtifactError::InvalidStorageKey);
        }
        Ok(self.root.join(package).join(version).join(filename))
    }
}

impl PackageArtifactStore for LocalFileArtifactStore {
    fn save(&self, request: PublishRequest<'_>) -> Result<StoredArtifact, ArtifactError> {
        let actual = sha256_hex(request.bytes);
        if actual != request.validated.checksum_sha256 {
            return Err(ArtifactError::ChecksumMismatch);
        }
        let package = storage_component(&request.validated.package_name);
        let version = storage_component(&request.validated.version);
        let storage_key = format!("{package}/{version}/artifact.bpk");
        let path = self.path_for_key(&storage_key)?;
        let parent = path.parent().expect("artifact path always has parent");
        fs::create_dir_all(parent).map_err(io_error)?;
        let temporary = parent.join(".artifact.bpk.tmp");
        fs::File::create(&temporary)
            .and_then(|mut file| file.write_all(request.bytes))
            .map_err(io_error)?;
        fs::rename(temporary, path).map_err(io_error)?;
        Ok(StoredArtifact {
            storage_key,
            checksum_sha256: actual,
            size_bytes: request.bytes.len() as u64,
        })
    }

    fn open(&self, storage_key: &str) -> Result<Vec<u8>, ArtifactError> {
        fs::read(self.path_for_key(storage_key)?).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ArtifactError::NotFound
            } else {
                io_error(error)
            }
        })
    }

    fn verify(&self, storage_key: &str, expected_sha256: &str) -> Result<bool, ArtifactError> {
        if !is_sha256(expected_sha256) {
            return Ok(false);
        }
        Ok(sha256_hex(&self.open(storage_key)?) == expected_sha256.to_ascii_lowercase())
    }

    fn delete(&self, storage_key: &str) -> Result<(), ArtifactError> {
        let path = self.path_for_key(storage_key)?;
        match fs::remove_file(&path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::remove_dir(parent);
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }
}

/// Parses and validates the `.bpk` format before persistence.
pub fn validate_package_artifact(
    bytes: &[u8],
    expected_package_name: &str,
    expected_version: &str,
) -> Result<ValidatedArtifact, ArtifactError> {
    if bytes.is_empty() {
        return Err(ArtifactError::EmptyArtifact);
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(ArtifactError::InvalidZip("too many entries".into()));
    }
    let mut entries = BTreeMap::new();
    let mut uncompressed = 0_u64;
    for index in 0..zip.len() {
        let entry = zip
            .by_index(index)
            .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = normalize_zip_path(entry.name())?;
        uncompressed = uncompressed.saturating_add(entry.size());
        if uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(ArtifactError::InvalidZip(
                "uncompressed size limit exceeded".into(),
            ));
        }
        if entries.insert(name.clone(), index).is_some() {
            return Err(ArtifactError::InvalidZip(format!(
                "duplicate entry '{name}'"
            )));
        }
    }
    for required in REQUIRED_ENTRIES {
        if !entries.contains_key(required) {
            return Err(ArtifactError::InvalidZip(format!(
                "missing required entry '{required}'"
            )));
        }
    }
    if !entries.keys().any(|path| path.starts_with("src/")) {
        return Err(ArtifactError::InvalidZip(
            "missing source file under src/".into(),
        ));
    }
    if entries.keys().any(|path| forbidden_path(path)) {
        return Err(ArtifactError::InvalidZip(
            "contains forbidden .beskid entry".into(),
        ));
    }
    let manifest_json = read_entry(&mut zip, entries["package.json"])?;
    validate_manifest(
        &manifest_json,
        expected_package_name,
        expected_version,
        &entries,
    )?;
    let project = read_entry(&mut zip, entries["Project.proj"])?;
    if !project
        .replace('\r', "")
        .contains(&format!("name = \"{expected_package_name}\""))
    {
        return Err(ArtifactError::InvalidManifest(
            "Project.proj package name does not match".into(),
        ));
    }
    let checksums = parse_checksums(&read_entry(&mut zip, entries["checksums.sha256"])?)?;
    if checksums.contains_key("checksums.sha256") {
        return Err(ArtifactError::InvalidChecksums(
            "checksums.sha256 must not reference itself".into(),
        ));
    }
    for (path, index) in &entries {
        if path == "checksums.sha256" {
            continue;
        }
        let expected = checksums.get(path).ok_or_else(|| {
            ArtifactError::InvalidChecksums(format!("missing checksum for '{path}'"))
        })?;
        let actual = sha256_hex(&read_entry_bytes(&mut zip, *index)?);
        if &actual != expected {
            return Err(ArtifactError::InvalidChecksums(format!(
                "checksum mismatch for '{path}'"
            )));
        }
    }
    for path in checksums.keys() {
        if !entries.contains_key(path) {
            return Err(ArtifactError::InvalidChecksums(format!(
                "references missing entry '{path}'"
            )));
        }
    }
    Ok(ValidatedArtifact {
        package_name: expected_package_name.to_owned(),
        version: expected_version.to_owned(),
        checksum_sha256: sha256_hex(bytes),
        size_bytes: bytes.len() as u64,
        manifest_json,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub version: String,
    pub is_yanked: bool,
}
impl ArtifactRecord {
    pub fn new(version: impl Into<String>, is_yanked: bool) -> Self {
        Self {
            version: version.into(),
            is_yanked,
        }
    }
}

/// Selects an exact non-yanked version, or the greatest semver for `latest`.
pub fn select_download<'a>(
    records: &'a [ArtifactRecord],
    requested: &str,
) -> Option<&'a ArtifactRecord> {
    if requested.eq_ignore_ascii_case("latest") {
        records
            .iter()
            .filter(|record| !record.is_yanked)
            .filter_map(|record| {
                Version::parse(&record.version)
                    .ok()
                    .map(|version| (version, record))
            })
            .max_by(|left, right| left.0.cmp(&right.0))
            .map(|(_, record)| record)
    } else {
        records
            .iter()
            .find(|record| !record.is_yanked && record.version == requested)
    }
}

fn validate_manifest(
    manifest: &str,
    package: &str,
    version: &str,
    entries: &BTreeMap<String, usize>,
) -> Result<(), ArtifactError> {
    let value: Value = serde_json::from_str(manifest)
        .map_err(|_| ArtifactError::InvalidManifest("package.json is not valid JSON".into()))?;
    let object = value.as_object().ok_or_else(|| {
        ArtifactError::InvalidManifest("package.json root must be an object".into())
    })?;
    let field = |name: &str| object.get(name).and_then(Value::as_str);
    if field("schema") != Some("beskid.package.v1") {
        return Err(ArtifactError::InvalidManifest(
            "schema must be beskid.package.v1".into(),
        ));
    }
    if !field("id").is_some_and(|id| id.eq_ignore_ascii_case(package)) {
        return Err(ArtifactError::InvalidManifest(
            "id does not match requested package".into(),
        ));
    }
    if field("version") != Some(version) {
        return Err(ArtifactError::InvalidManifest(
            "version does not match requested version".into(),
        ));
    }
    let kind = field("packageKind").unwrap_or("library");
    if !matches!(kind, "library" | "template" | "tool") {
        return Err(ArtifactError::InvalidManifest(
            "packageKind is unsupported".into(),
        ));
    }
    if kind == "template" && !entries.contains_key("template.json") {
        return Err(ArtifactError::InvalidManifest(
            "template package requires template.json".into(),
        ));
    }
    if kind != "template" && entries.contains_key("template.json") {
        return Err(ArtifactError::InvalidManifest(
            "only templates may include template.json".into(),
        ));
    }
    if let Some(dependencies) = object.get("dependencies").and_then(Value::as_array) {
        for dependency in dependencies {
            let source = dependency
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("registry");
            if !matches!(source, "registry" | "pckg") {
                return Err(ArtifactError::InvalidManifest(
                    "published dependency must use registry source".into(),
                ));
            }
            if dependency
                .get("version")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(ArtifactError::InvalidManifest(
                    "published dependency must have a version".into(),
                ));
            }
        }
    }
    Ok(())
}

fn parse_checksums(contents: &str) -> Result<BTreeMap<String, String>, ArtifactError> {
    let mut checksums = BTreeMap::new();
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let mut chunks = line.split_whitespace();
        let digest = chunks
            .next()
            .ok_or_else(|| ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")))?;
        let path = chunks
            .next_back()
            .ok_or_else(|| ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")))?;
        if chunks.next().is_some() || !is_sha256(digest) {
            return Err(ArtifactError::InvalidChecksums(format!(
                "invalid entry '{line}'"
            )));
        }
        checksums.insert(normalize_zip_path(path)?, digest.to_ascii_lowercase());
    }
    Ok(checksums)
}

fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<String, ArtifactError> {
    String::from_utf8(read_entry_bytes(zip, index)?)
        .map_err(|_| ArtifactError::InvalidZip("text entry is not UTF-8".into()))
}
fn read_entry_bytes(
    zip: &mut ZipArchive<Cursor<&[u8]>>,
    index: usize,
) -> Result<Vec<u8>, ArtifactError> {
    let mut bytes = Vec::new();
    zip.by_index(index)
        .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    Ok(bytes)
}
fn read_entry_limited(
    zip: &mut ZipArchive<Cursor<&[u8]>>,
    index: usize,
    path: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let entry = zip
        .by_index(index)
        .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
    let mut bytes = Vec::with_capacity(entry.size().min(MAX_BROWSE_READ_BYTES) as usize);
    entry
        .take(MAX_BROWSE_READ_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_BROWSE_READ_BYTES {
        return Err(ArtifactError::EntryTooLarge {
            path: path.to_owned(),
            limit_bytes: MAX_BROWSE_READ_BYTES,
        });
    }
    Ok(bytes)
}
fn normalize_zip_path(path: &str) -> Result<String, ArtifactError> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ArtifactError::InvalidZip(format!(
            "unsafe entry path '{path}'"
        )));
    }
    Ok(path)
}
fn normalize_browse_request(path: &str) -> Result<String, ArtifactError> {
    normalize_zip_path(path).map_err(|_| ArtifactError::ForbiddenBrowsePath)
}
fn is_documentation_path(path: &str) -> bool {
    path == "README.md" || path.starts_with("docs/") || path.starts_with(".beskid/docs/")
}
fn is_source_path(path: &str) -> bool {
    path.starts_with("src/")
}
fn is_browseable_archive_entry(path: &str) -> bool {
    if matches!(path, "package.json" | "Project.proj" | "checksums.sha256") {
        return true;
    }
    if is_documentation_path(path) || is_source_path(path) {
        return !path.split('/').any(|segment| {
            segment.starts_with('.') && !(segment == ".beskid" && path.starts_with(".beskid/"))
        });
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
fn forbidden_path(path: &str) -> bool {
    path.starts_with(".beskid/") && !path.starts_with(".beskid/docs/")
}
fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn storage_component(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
fn is_storage_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= 200
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && component != "."
        && component != ".."
}
fn io_error(error: std::io::Error) -> ArtifactError {
    ArtifactError::Io(error.to_string())
}
