use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::abi_v5::{AbiManifestV5, RuntimeAuditMetadata, TargetMetadata, TargetValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    /// Parse an explicit runtime-kit coordinate without selecting another profile on invalid input.
    pub fn parse(value: &str) -> Result<Self, InvalidBuildProfile> {
        match value {
            "debug" => Ok(Self::Debug),
            "release" => Ok(Self::Release),
            _ => Err(InvalidBuildProfile { value: value.to_owned() }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidBuildProfile {
    value: String,
}

impl std::fmt::Display for InvalidBuildProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid BESKID_RUNTIME_KIT_PROFILE `{}`; expected `debug` or `release`", self.value)
    }
}

impl std::error::Error for InvalidBuildProfile {}

#[cfg(test)]
mod profile_tests {
    use super::BuildProfile;

    #[test]
    fn explicit_runtime_kit_profiles_parse_exactly() {
        assert_eq!(BuildProfile::parse("debug"), Ok(BuildProfile::Debug));
        assert_eq!(BuildProfile::parse("release"), Ok(BuildProfile::Release));
    }

    #[test]
    fn invalid_runtime_kit_profile_is_rejected() {
        let error = BuildProfile::parse("relase").expect_err("profile typo must fail closed");
        assert!(error.to_string().contains("expected `debug` or `release`"));
        assert!(BuildProfile::parse("").is_err());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifact {
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifacts {
    pub static_library: RuntimeArtifact,
    pub shared_library: RuntimeArtifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_import_library: Option<RuntimeArtifact>,
}

impl RuntimeArtifacts {
    pub(super) fn iter(&self) -> impl Iterator<Item = &RuntimeArtifact> {
        [&self.static_library, &self.shared_library].into_iter().chain(self.shared_import_library.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeKitMetadata {
    pub schema_version: u32,
    pub abi_version: u32,
    pub target: TargetMetadata,
    pub profile: BuildProfile,
    pub layout_hash: String,
    pub source_hash: String,
    pub artifacts: RuntimeArtifacts,
    pub import_allowlist: Vec<String>,
    pub export_allowlist: Vec<String>,
    pub loader_required_exports: Vec<String>,
    pub abi_contract: AbiManifestV5,
    pub audit: RuntimeAuditMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeKitValidationError {
    WrongSchemaVersion(u32),
    WrongAbiVersion(u32),
    InvalidTarget(TargetValidationError),
    InvalidSha256 { field: String },
    InvalidArtifactSet { target: String },
    InvalidArtifactPath(String),
    DuplicateAllowlistSymbol { symbol: String },
    InvalidAbiContract,
    ContractTargetMismatch,
    ContractLayoutHashMismatch { actual: String },
    ContractSourceHashMismatch { actual: String },
    ContractAuditMismatch { field: String },
}

#[derive(Debug)]
pub struct ResolvedRuntimeKit {
    pub root: PathBuf,
    pub metadata: RuntimeKitMetadata,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

#[derive(Debug)]
pub enum RuntimeKitResolutionError {
    RequestedTarget(TargetValidationError),
    MetadataRead { path: PathBuf, source: std::io::Error },
    MetadataDecode { path: PathBuf, source: serde_json::Error },
    MetadataValidation(RuntimeKitValidationError),
    TargetMismatch { requested: String, actual: String },
    ProfileMismatch { requested: BuildProfile, actual: BuildProfile },
    ArtifactRead { path: PathBuf, source: std::io::Error },
    ArtifactNotRegularFile { path: PathBuf },
    ArtifactHashMismatch { path: PathBuf, expected: String, actual: String },
}

#[derive(Debug, Clone)]
pub struct RuntimeKitBuildRequest {
    pub prefix: PathBuf,
    pub target: TargetMetadata,
    pub profile: BuildProfile,
    pub runtime_source_hash: String,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

#[derive(Debug)]
pub enum RuntimeKitBuildError {
    InvalidTarget(TargetValidationError),
    InvalidSourceHash,
    InvalidArtifactSet { target: String },
    SourceArtifactRead { path: PathBuf, source: std::io::Error },
    SourceArtifactNotRegularFile { path: PathBuf },
    DestinationExists { path: PathBuf },
    DestinationWrite { path: PathBuf, source: std::io::Error },
    CopiedArtifactHashMismatch { path: PathBuf, expected: String, actual: String },
    Metadata(RuntimeKitValidationError),
    Resolution(RuntimeKitResolutionError),
}
