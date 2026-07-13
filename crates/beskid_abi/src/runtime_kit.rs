//! Serializable metadata for installed native ABI-v5 runtime kits.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::abi_v5::{
    ABI_V5, AbiManifestV5, RuntimeAuditMetadata, TargetMetadata, TargetValidationError,
};

pub const RUNTIME_KIT_SCHEMA_VERSION: u32 = 1;

const INSTALLED_RUNTIME_ROOT: &str = "lib/beskid-runtime/abi-5";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
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
    fn iter(&self) -> impl Iterator<Item = &RuntimeArtifact> {
        [&self.static_library, &self.shared_library]
            .into_iter()
            .chain(self.shared_import_library.iter())
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
    pub abi_contract: AbiManifestV5,
    pub audit: RuntimeAuditMetadata,
}

impl RuntimeKitMetadata {
    pub fn canonical_abi_json(&self) -> Result<String, RuntimeKitValidationError> {
        self.validate()?;
        let mut output = serde_json::to_string_pretty(self)
            .map_err(|_| RuntimeKitValidationError::InvalidAbiContract)?;
        output.push('\n');
        Ok(output)
    }

    pub fn validate(&self) -> Result<(), RuntimeKitValidationError> {
        if self.schema_version != RUNTIME_KIT_SCHEMA_VERSION {
            return Err(RuntimeKitValidationError::WrongSchemaVersion(
                self.schema_version,
            ));
        }
        if self.abi_version != ABI_V5 {
            return Err(RuntimeKitValidationError::WrongAbiVersion(self.abi_version));
        }
        self.target
            .validate()
            .map_err(RuntimeKitValidationError::InvalidTarget)?;
        self.abi_contract
            .validate()
            .map_err(|_| RuntimeKitValidationError::InvalidAbiContract)?;
        if self.abi_contract.target != self.target
            || self.abi_contract.abi_version != self.abi_version
        {
            return Err(RuntimeKitValidationError::ContractTargetMismatch);
        }
        if self.abi_contract != AbiManifestV5::canonical_runtime(self.target.clone()) {
            return Err(RuntimeKitValidationError::InvalidAbiContract);
        }
        for (name, hash) in [
            ("layout_hash", &self.layout_hash),
            ("source_hash", &self.source_hash),
        ] {
            validate_sha256(name, hash)?;
        }

        for artifact in self.artifacts.iter() {
            if !is_portable_relative_path(&artifact.relative_path) {
                return Err(RuntimeKitValidationError::InvalidArtifactPath(
                    artifact.relative_path.clone(),
                ));
            }
            validate_sha256("artifact.sha256", &artifact.sha256)?;
        }

        let (static_path, shared_path, import_path) = match self.target.object_format.as_str() {
            "elf" => (
                "static/libbeskid_runtime.a",
                "shared/libbeskid_runtime.so",
                None,
            ),
            "macho" => (
                "static/libbeskid_runtime.a",
                "shared/libbeskid_runtime.dylib",
                None,
            ),
            "coff" => (
                "static/beskid_runtime.lib",
                "shared/beskid_runtime.dll",
                Some("shared/beskid_runtime_import.lib"),
            ),
            _ => unreachable!("target validation rejects unsupported object formats"),
        };
        let actual_import_path = self
            .artifacts
            .shared_import_library
            .as_ref()
            .map(|artifact| artifact.relative_path.as_str());
        if self.artifacts.static_library.relative_path != static_path
            || self.artifacts.shared_library.relative_path != shared_path
            || actual_import_path != import_path
        {
            return Err(RuntimeKitValidationError::InvalidArtifactSet {
                target: self.target.triple.as_str().into(),
            });
        }

        validate_allowlist(&self.import_allowlist)?;
        validate_allowlist(&self.export_allowlist)?;
        if self.layout_hash != self.abi_contract.layout_hash()
            || self.layout_hash != self.audit.layout_hash
        {
            return Err(RuntimeKitValidationError::ContractLayoutHashMismatch {
                actual: self.layout_hash.clone(),
            });
        }
        if self.source_hash != self.audit.runtime_source_hash {
            return Err(RuntimeKitValidationError::ContractSourceHashMismatch {
                actual: self.source_hash.clone(),
            });
        }
        self.audit.validate(&self.abi_contract).map_err(|_| {
            RuntimeKitValidationError::ContractAuditMismatch {
                field: "audit".into(),
            }
        })?;
        if self.import_allowlist != self.audit.allowed_imports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch {
                field: "import_allowlist".into(),
            });
        }
        if self.export_allowlist != self.audit.allowed_exports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch {
                field: "export_allowlist".into(),
            });
        }
        Ok(())
    }
}

fn is_portable_relative_path(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value.as_bytes().get(1) == Some(&b':')
    {
        return false;
    }
    value
        .split('/')
        .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn validate_sha256(name: &str, value: &str) -> Result<(), RuntimeKitValidationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RuntimeKitValidationError::InvalidSha256 { field: name.into() });
    }
    Ok(())
}

fn validate_allowlist(symbols: &[String]) -> Result<(), RuntimeKitValidationError> {
    let mut seen = HashSet::new();
    for symbol in symbols {
        if symbol.is_empty() || !seen.insert(symbol.as_str()) {
            return Err(RuntimeKitValidationError::DuplicateAllowlistSymbol {
                symbol: symbol.clone(),
            });
        }
    }
    Ok(())
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
    MetadataRead {
        path: PathBuf,
        source: std::io::Error,
    },
    MetadataDecode {
        path: PathBuf,
        source: serde_json::Error,
    },
    MetadataValidation(RuntimeKitValidationError),
    TargetMismatch {
        requested: String,
        actual: String,
    },
    ProfileMismatch {
        requested: BuildProfile,
        actual: BuildProfile,
    },
    ArtifactRead {
        path: PathBuf,
        source: std::io::Error,
    },
    ArtifactNotRegularFile {
        path: PathBuf,
    },
    ArtifactHashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
}

pub fn resolve_installed_runtime_kit(
    prefix: &Path,
    target: &TargetMetadata,
    profile: BuildProfile,
) -> Result<ResolvedRuntimeKit, RuntimeKitResolutionError> {
    target
        .validate()
        .map_err(RuntimeKitResolutionError::RequestedTarget)?;
    let profile_directory = match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    };
    let root = prefix
        .join(INSTALLED_RUNTIME_ROOT)
        .join(target.triple.as_str())
        .join(profile_directory);
    let metadata_path = root.join("abi.json");
    let metadata_json = fs::read_to_string(&metadata_path).map_err(|source| {
        RuntimeKitResolutionError::MetadataRead {
            path: metadata_path.clone(),
            source,
        }
    })?;
    let metadata: RuntimeKitMetadata = serde_json::from_str(&metadata_json).map_err(|source| {
        RuntimeKitResolutionError::MetadataDecode {
            path: metadata_path.clone(),
            source,
        }
    })?;
    metadata
        .validate()
        .map_err(RuntimeKitResolutionError::MetadataValidation)?;
    if metadata.target != *target {
        return Err(RuntimeKitResolutionError::TargetMismatch {
            requested: target.triple.as_str().into(),
            actual: metadata.target.triple.as_str().into(),
        });
    }
    if metadata.profile != profile {
        return Err(RuntimeKitResolutionError::ProfileMismatch {
            requested: profile,
            actual: metadata.profile,
        });
    }

    let static_library = verify_artifact(&root, &metadata.artifacts.static_library)?;
    let shared_library = verify_artifact(&root, &metadata.artifacts.shared_library)?;
    let shared_import_library = metadata
        .artifacts
        .shared_import_library
        .as_ref()
        .map(|artifact| verify_artifact(&root, artifact))
        .transpose()?;

    Ok(ResolvedRuntimeKit {
        root,
        metadata,
        static_library,
        shared_library,
        shared_import_library,
    })
}

fn verify_artifact(
    root: &Path,
    artifact: &RuntimeArtifact,
) -> Result<PathBuf, RuntimeKitResolutionError> {
    let path = root.join(&artifact.relative_path);
    let file_type = fs::symlink_metadata(&path)
        .map_err(|source| RuntimeKitResolutionError::ArtifactRead {
            path: path.clone(),
            source,
        })?
        .file_type();
    if !file_type.is_file() {
        return Err(RuntimeKitResolutionError::ArtifactNotRegularFile { path });
    }
    let mut file = File::open(&path).map_err(|source| RuntimeKitResolutionError::ArtifactRead {
        path: path.clone(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read =
            file.read(&mut buffer)
                .map_err(|source| RuntimeKitResolutionError::ArtifactRead {
                    path: path.clone(),
                    source,
                })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != artifact.sha256 {
        return Err(RuntimeKitResolutionError::ArtifactHashMismatch {
            path,
            expected: artifact.sha256.clone(),
            actual,
        });
    }
    Ok(path)
}
