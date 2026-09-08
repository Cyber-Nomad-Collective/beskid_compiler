//! Safe package artifact validation and local artifact storage.
//!
//! This crate owns byte-level invariants only.  Package authorization,
//! publication idempotency, and version metadata remain server/store concerns.

use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use bsol::{BsolBlock, BsolItem, BsolValue, parse_bsol_document};
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
const REQUIRED_ENTRIES: [&str; 2] = ["package.json", "checksums.sha256"];

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
    pub metadata: PackageManifestMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub storage_key: String,
    pub checksum_sha256: String,
    pub size_bytes: u64,
}

/// The only dependency shape permitted in a published package artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDependency {
    pub name: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Library,
    Template,
    Tool,
}

impl PackageKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Template => "template",
            Self::Tool => "tool",
        }
    }

    fn parse(value: &str) -> Result<Self, ArtifactError> {
        match value {
            "library" => Ok(Self::Library),
            "template" => Ok(Self::Template),
            "tool" => Ok(Self::Tool),
            _ => Err(ArtifactError::InvalidManifest("packageKind is unsupported".into())),
        }
    }
}

/// Canonical metadata parsed from artifact-root `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifestMetadata {
    pub package_kind: PackageKind,
    pub template: Option<Value>,
    pub dependencies: Vec<ArtifactDependency>,
}

impl ArtifactDependency {
    pub fn registry(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self { name: name.into(), version: version.into(), source: "registry".into() }
    }
}

#[derive(Debug, Clone)]
struct ProjectDependencyBlock {
    dependency: ArtifactDependency,
    span_start: usize,
    span_end: usize,
}

/// Rewrites source-only dependency declarations into path-independent registry
/// declarations for the artifact. The source manifest passed by the caller is
/// never changed on disk.
pub fn canonicalize_project_dependencies(
    project: &str,
    planned: &[ArtifactDependency],
) -> Result<(String, Vec<ArtifactDependency>), ArtifactError> {
    let planned = dependency_map(planned, "dependency plan")?;
    let blocks = parse_project_dependencies(project)?;
    let mut resolved = Vec::with_capacity(blocks.len());
    for block in &blocks {
        let declared = &block.dependency;
        let dependency = if declared.source == "registry" {
            validate_exact_version(&declared.version, &declared.name)?;
            if let Some(planned) = planned.get(&declared.name)
                && planned.version != declared.version
            {
                return Err(ArtifactError::InvalidManifest(format!(
                    "dependency plan version for '{}' disagrees with the project manifest",
                    declared.name
                )));
            }
            ArtifactDependency::registry(&declared.name, &declared.version)
        } else {
            planned.get(&declared.name).cloned().ok_or_else(|| {
                ArtifactError::InvalidManifest(format!(
                    "dependency '{}' uses source '{}' but has no exact registry version in package.json",
                    declared.name, declared.source
                ))
            })?
        };
        resolved.push(dependency);
    }
    let resolved_map = dependency_map(&resolved, "project dependencies")?;
    if planned.keys().any(|name| !resolved_map.contains_key(name)) {
        return Err(ArtifactError::InvalidManifest(
            "dependency plan contains a package not declared by the project manifest".into(),
        ));
    }

    let mut rewritten = project.to_owned();
    for (block, dependency) in blocks.iter().zip(&resolved).rev() {
        let replacement = format!(
            "dependency \"{}\" {{\n  source = registry\n  version = \"{}\"\n}}",
            dependency.name, dependency.version
        );
        rewritten.replace_range(block.span_start..block.span_end, &replacement);
    }
    resolved.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((rewritten, resolved))
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
        if filename != "artifact.bpk" || !is_storage_component(package) || !is_storage_component(version) {
            return Err(ArtifactError::InvalidStorageKey);
        }
        Ok(self.root.join(package).join(version).join(filename))
    }

    /// Stages immutable artifact bytes without ever replacing an existing
    /// deterministic package/version object. The boolean is true only when
    /// this call created the object, so a caller can safely compensate a later
    /// metadata-transaction failure without deleting another publisher's work.
    pub fn save_staged(&self, request: PublishRequest<'_>) -> Result<(StoredArtifact, bool), ArtifactError> {
        let actual = sha256_hex(request.bytes);
        if actual != request.validated.checksum_sha256 {
            return Err(ArtifactError::ChecksumMismatch);
        }
        let package = storage_component(&request.validated.package_name);
        let version = storage_component(&request.validated.version);
        let storage_key = format!("{package}/{version}/artifact.bpk");
        let path = self.path_for_key(&storage_key)?;
        let stored =
            StoredArtifact { storage_key, checksum_sha256: actual.clone(), size_bytes: request.bytes.len() as u64 };
        match fs::read(&path) {
            Ok(existing) => {
                return if sha256_hex(&existing) == actual {
                    Ok((stored, false))
                } else {
                    Err(ArtifactError::ChecksumMismatch)
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
        let parent = path.parent().expect("artifact path always has parent");
        fs::create_dir_all(parent).map_err(io_error)?;
        static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let temporary = parent.join(format!(
            ".artifact.bpk.{}.{}.tmp",
            std::process::id(),
            TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::File::create_new(&temporary).and_then(|mut file| file.write_all(request.bytes)).map_err(io_error)?;
        match fs::hard_link(&temporary, &path) {
            Ok(()) => {
                let _ = fs::remove_file(&temporary);
                Ok((stored, true))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temporary);
                let existing = fs::read(&path).map_err(io_error)?;
                if sha256_hex(&existing) == actual { Ok((stored, false)) } else { Err(ArtifactError::ChecksumMismatch) }
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(io_error(error))
            }
        }
    }
}

impl PackageArtifactStore for LocalFileArtifactStore {
    fn save(&self, request: PublishRequest<'_>) -> Result<StoredArtifact, ArtifactError> {
        self.save_staged(request).map(|(stored, _)| stored)
    }

    fn open(&self, storage_key: &str) -> Result<Vec<u8>, ArtifactError> {
        fs::read(self.path_for_key(storage_key)?).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound { ArtifactError::NotFound } else { io_error(error) }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub version: String,
    pub is_yanked: bool,
}
impl ArtifactRecord {
    pub fn new(version: impl Into<String>, is_yanked: bool) -> Self {
        Self { version: version.into(), is_yanked }
    }
}

/// Selects an exact non-yanked version, or the greatest semver for `latest`.
pub fn select_download<'a>(records: &'a [ArtifactRecord], requested: &str) -> Option<&'a ArtifactRecord> {
    if requested.eq_ignore_ascii_case("latest") {
        records
            .iter()
            .filter(|record| !record.is_yanked)
            .filter_map(|record| Version::parse(&record.version).ok().map(|version| (version, record)))
            .max_by(|left, right| left.0.cmp(&right.0))
            .map(|(_, record)| record)
    } else {
        records.iter().find(|record| !record.is_yanked && record.version == requested)
    }
}

fn validate_manifest(
    manifest: &str,
    package: &str,
    version: &str,
    entries: &BTreeMap<String, usize>,
) -> Result<PackageManifestMetadata, ArtifactError> {
    let metadata = parse_package_manifest_metadata(manifest, package, version)?;
    if metadata.package_kind == PackageKind::Template && !entries.contains_key("template.json") {
        return Err(ArtifactError::InvalidManifest("template package requires template.json".into()));
    }
    if metadata.package_kind != PackageKind::Template && entries.contains_key("template.json") {
        return Err(ArtifactError::InvalidManifest("only templates may include template.json".into()));
    }
    Ok(metadata)
}

/// Parses the immutable manifest metadata persisted beside a package version.
/// Artifact validation and catalog projection share this one interpretation.
pub fn parse_package_manifest_metadata(
    manifest: &str,
    package: &str,
    version: &str,
) -> Result<PackageManifestMetadata, ArtifactError> {
    let value: Value = serde_json::from_str(manifest)
        .map_err(|_| ArtifactError::InvalidManifest("package.json is not valid JSON".into()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ArtifactError::InvalidManifest("package.json root must be an object".into()))?;
    let field = |name: &str| object.get(name).and_then(Value::as_str);
    if field("schema") != Some("beskid.package.v1") {
        return Err(ArtifactError::InvalidManifest("schema must be beskid.package.v1".into()));
    }
    if !field("id").is_some_and(|id| id.eq_ignore_ascii_case(package)) {
        return Err(ArtifactError::InvalidManifest("id does not match requested package".into()));
    }
    if field("version") != Some(version) {
        return Err(ArtifactError::InvalidManifest("version does not match requested version".into()));
    }
    let kind = PackageKind::parse(
        field("packageKind").ok_or_else(|| ArtifactError::InvalidManifest("packageKind is required".into()))?,
    )?;
    let dependencies = match object.get("dependencies") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|dependency| {
                let name = dependency.get("name").and_then(Value::as_str).unwrap_or_default();
                let version = dependency.get("version").and_then(Value::as_str).unwrap_or_default();
                let source = dependency.get("source").and_then(Value::as_str).unwrap_or_default();
                if name.is_empty() {
                    return Err(ArtifactError::InvalidManifest("published dependency must have a name".into()));
                }
                if source != "registry" {
                    return Err(ArtifactError::InvalidManifest(
                        "published dependency must use canonical registry source".into(),
                    ));
                }
                validate_exact_version(version, name)?;
                Ok(ArtifactDependency::registry(name, version))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(ArtifactError::InvalidManifest("dependencies must be an array".into()));
        }
    };
    dependency_map(&dependencies, "package.json dependencies")?;
    let template = object.get("template").cloned();
    if kind == PackageKind::Template && !template.as_ref().is_some_and(Value::is_object) {
        return Err(ArtifactError::InvalidManifest("template package requires a template summary object".into()));
    }
    Ok(PackageManifestMetadata { package_kind: kind, dependencies, template })
}

fn dependency_map(
    dependencies: &[ArtifactDependency],
    context: &str,
) -> Result<BTreeMap<String, ArtifactDependency>, ArtifactError> {
    let mut map = BTreeMap::new();
    for dependency in dependencies {
        if dependency.name.is_empty() || dependency.source != "registry" {
            return Err(ArtifactError::InvalidManifest(format!("{context} must contain named registry dependencies")));
        }
        validate_exact_version(&dependency.version, &dependency.name)?;
        if map.insert(dependency.name.clone(), dependency.clone()).is_some() {
            return Err(ArtifactError::InvalidManifest(format!(
                "{context} contains duplicate dependency '{}'",
                dependency.name
            )));
        }
    }
    Ok(map)
}

fn validate_exact_version(version: &str, dependency: &str) -> Result<(), ArtifactError> {
    Version::parse(version).map_err(|_| {
        ArtifactError::InvalidManifest(format!(
            "published dependency '{dependency}' must have an exact semantic version"
        ))
    })?;
    Ok(())
}

fn parse_project_dependencies(project: &str) -> Result<Vec<ProjectDependencyBlock>, ArtifactError> {
    let document = parse_bsol_document(project)
        .map_err(|error| ArtifactError::InvalidManifest(format!("project manifest is invalid: {error}")))?;
    document
        .blocks
        .iter()
        .filter(|block| block.kind == "dependency")
        .map(|block| {
            let name =
                block.label.as_ref().map(|label| label.value.clone()).filter(|name| !name.is_empty()).ok_or_else(
                    || ArtifactError::InvalidManifest("dependency block must have a package name".into()),
                )?;
            let source = block_string_field(block, "source").unwrap_or_default();
            let version = block_string_field(block, "version").unwrap_or_default();
            Ok(ProjectDependencyBlock {
                dependency: ArtifactDependency { name, version, source },
                span_start: block.span.start,
                span_end: block.span.end,
            })
        })
        .collect()
}

fn block_string_field(block: &BsolBlock, field: &str) -> Option<String> {
    block.items.iter().find_map(|item| match item {
        BsolItem::Assignment(assignment) if assignment.key == field => match &assignment.value {
            BsolValue::QuotedString(value) => Some(value.value.clone()),
            BsolValue::Ident(value) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    })
}

fn validate_published_project_dependencies(
    project: &str,
    manifest_dependencies: &[ArtifactDependency],
) -> Result<(), ArtifactError> {
    let blocks = parse_project_dependencies(project)?;
    let declared = blocks.into_iter().map(|block| block.dependency).collect::<Vec<_>>();
    for dependency in &declared {
        if dependency.source != "registry" {
            return Err(ArtifactError::InvalidManifest(format!(
                "published project dependency '{}' must use registry source",
                dependency.name
            )));
        }
        validate_exact_version(&dependency.version, &dependency.name)?;
    }
    if dependency_map(&declared, "project dependencies")?
        != dependency_map(manifest_dependencies, "package.json dependencies")?
    {
        return Err(ArtifactError::InvalidManifest(
            "project dependencies disagree with package.json dependencies".into(),
        ));
    }
    Ok(())
}

fn validate_template_contract(
    template_json: &str,
    package: &str,
    package_summary: Option<&Value>,
) -> Result<(), ArtifactError> {
    let template: Value = serde_json::from_str(template_json)
        .map_err(|_| ArtifactError::InvalidManifest("template.json is not valid JSON".into()))?;
    let object = template
        .as_object()
        .ok_or_else(|| ArtifactError::InvalidManifest("template.json root must be an object".into()))?;
    if object.get("schema").and_then(Value::as_str) != Some("beskid.template.v1") {
        return Err(ArtifactError::InvalidManifest("template.json schema must be beskid.template.v1".into()));
    }
    let identity = object.get("identity").and_then(Value::as_str).unwrap_or_default();
    if identity.split_once("::").map_or(identity, |(name, _)| name) != package {
        return Err(ArtifactError::InvalidManifest("template.json identity does not match package identity".into()));
    }
    let mut expected_summary = serde_json::Map::new();
    for key in ["shortName", "identity", "tags"] {
        if let Some(value) = object.get(key) {
            expected_summary.insert(key.into(), value.clone());
        }
    }
    if package_summary != Some(&Value::Object(expected_summary)) {
        return Err(ArtifactError::InvalidManifest(
            "package.json template summary disagrees with template.json".into(),
        ));
    }
    Ok(())
}

fn project_field<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    content.lines().map(str::trim).filter(|line| !line.starts_with('#')).find_map(|line| {
        let (current, value) = line.split_once('=')?;
        if current.trim() != key {
            return None;
        }
        Some(value.trim().trim_matches('"'))
    })
}

fn project_root_block_identifier(content: &str) -> Option<&str> {
    let line = content.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with('#'))?;
    let identifier = line.strip_suffix('{')?.trim();
    let mut characters = identifier.chars();
    let first = characters.next()?;
    if !(first.is_ascii_alphabetic() || first == '_')
        || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some(identifier)
}

fn parse_checksums(contents: &str) -> Result<BTreeMap<String, String>, ArtifactError> {
    let mut checksums = BTreeMap::new();
    for line in contents.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let mut chunks = line.split_whitespace();
        let digest = chunks.next().ok_or_else(|| ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")))?;
        let path =
            chunks.next_back().ok_or_else(|| ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")))?;
        if chunks.next().is_some() || !is_sha256(digest) {
            return Err(ArtifactError::InvalidChecksums(format!("invalid entry '{line}'")));
        }
        checksums.insert(normalize_zip_path(path)?, digest.to_ascii_lowercase());
    }
    Ok(checksums)
}

fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<String, ArtifactError> {
    String::from_utf8(read_entry_bytes(zip, index)?)
        .map_err(|_| ArtifactError::InvalidZip("text entry is not UTF-8".into()))
}
fn read_entry_bytes(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<Vec<u8>, ArtifactError> {
    let mut bytes = Vec::new();
    zip.by_index(index)
        .map_err(|error| ArtifactError::InvalidZip(error.to_string()))?
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    Ok(bytes)
}
fn read_entry_limited(zip: &mut ZipArchive<Cursor<&[u8]>>, index: usize, path: &str) -> Result<Vec<u8>, ArtifactError> {
    let entry = zip.by_index(index).map_err(|error| ArtifactError::InvalidZip(error.to_string()))?;
    let mut bytes = Vec::with_capacity(entry.size().min(MAX_BROWSE_READ_BYTES) as usize);
    entry.take(MAX_BROWSE_READ_BYTES + 1).read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() as u64 > MAX_BROWSE_READ_BYTES {
        return Err(ArtifactError::EntryTooLarge { path: path.to_owned(), limit_bytes: MAX_BROWSE_READ_BYTES });
    }
    Ok(bytes)
}
fn normalize_zip_path(path: &str) -> Result<String, ArtifactError> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ArtifactError::InvalidZip(format!("unsafe entry path '{path}'")));
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
        || path.starts_with("content/")
        || path.starts_with("workspace/")
        || path.starts_with("item/")
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
        .map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') { ch } else { '_' })
        .collect()
}
fn is_storage_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= 200
        && component.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && component != "."
        && component != ".."
}
fn io_error(error: std::io::Error) -> ArtifactError {
    ArtifactError::Io(error.to_string())
}
