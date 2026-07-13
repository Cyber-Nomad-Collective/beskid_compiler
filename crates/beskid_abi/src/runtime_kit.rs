//! Serializable metadata for installed native ABI-v5 runtime kits.

use std::collections::HashSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::abi_v5::{ABI_V5, RUNTIME_SYMBOL_PREFIX, TargetMetadata, TargetValidationError};

pub const RUNTIME_KIT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactLinkage {
    Static,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeArtifact {
    pub profile: BuildProfile,
    pub linkage: ArtifactLinkage,
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeKitMetadata {
    pub schema_version: u32,
    pub abi_version: u32,
    pub target: TargetMetadata,
    pub layout_hash: String,
    pub source_hash: String,
    pub artifacts: Vec<RuntimeArtifact>,
    pub import_allowlist: Vec<String>,
    pub export_allowlist: Vec<String>,
}

impl RuntimeKitMetadata {
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
        for (name, hash) in [
            ("layout_hash", &self.layout_hash),
            ("source_hash", &self.source_hash),
        ] {
            validate_sha256(name, hash)?;
        }

        let expected: HashSet<_> = [
            (BuildProfile::Debug, ArtifactLinkage::Static),
            (BuildProfile::Debug, ArtifactLinkage::Shared),
            (BuildProfile::Release, ArtifactLinkage::Static),
            (BuildProfile::Release, ArtifactLinkage::Shared),
        ]
        .into_iter()
        .collect();
        let actual: HashSet<_> = self
            .artifacts
            .iter()
            .map(|artifact| (artifact.profile, artifact.linkage))
            .collect();
        if self.artifacts.len() != expected.len() || actual != expected {
            return Err(RuntimeKitValidationError::InvalidArtifactMatrix {
                artifact_count: self.artifacts.len(),
            });
        }
        for artifact in &self.artifacts {
            let path = Path::new(&artifact.relative_path);
            if artifact.relative_path.is_empty()
                || path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
            {
                return Err(RuntimeKitValidationError::InvalidArtifactPath(
                    artifact.relative_path.clone(),
                ));
            }
            validate_sha256("artifact.sha256", &artifact.sha256)?;
        }

        validate_allowlist(&self.import_allowlist)?;
        validate_allowlist(&self.export_allowlist)?;
        if let Some(symbol) = self
            .export_allowlist
            .iter()
            .find(|symbol| !symbol.starts_with(RUNTIME_SYMBOL_PREFIX))
        {
            return Err(RuntimeKitValidationError::UnversionedExportSymbol(
                symbol.clone(),
            ));
        }
        Ok(())
    }
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
    InvalidArtifactMatrix { artifact_count: usize },
    InvalidArtifactPath(String),
    DuplicateAllowlistSymbol { symbol: String },
    UnversionedExportSymbol(String),
}
