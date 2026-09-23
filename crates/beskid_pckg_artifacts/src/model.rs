use semver::Version;
use serde_json::Value;

use crate::errors::ArtifactError;

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

impl ArtifactDependency {
    pub fn registry(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self { name: name.into(), version: version.into(), source: "registry".into() }
    }
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

    pub(crate) fn parse(value: &str) -> Result<Self, ArtifactError> {
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

pub struct PublishRequest<'a> {
    pub validated: ValidatedArtifact,
    pub bytes: &'a [u8],
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
