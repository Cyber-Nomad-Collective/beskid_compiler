//! Serializable metadata for installed native ABI-v5 runtime kits.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::abi_v5::{
    ABI_V5, RUNTIME_SYMBOL_PREFIX, TargetMetadata, TargetTriple, TargetValidationError,
};

pub const RUNTIME_KIT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeArtifact {
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

        for artifact in self.artifacts.iter() {
            if !is_portable_relative_path(&artifact.relative_path) {
                return Err(RuntimeKitValidationError::InvalidArtifactPath(
                    artifact.relative_path.clone(),
                ));
            }
            validate_sha256("artifact.sha256", &artifact.sha256)?;
        }

        let (static_path, shared_path, import_path) = match self.target.triple {
            TargetTriple::X86_64UnknownLinuxGnu => (
                "static/libbeskid_runtime.a",
                "shared/libbeskid_runtime.so",
                None,
            ),
            TargetTriple::Aarch64AppleDarwin => (
                "static/libbeskid_runtime.a",
                "shared/libbeskid_runtime.dylib",
                None,
            ),
            TargetTriple::X86_64PcWindowsMsvc => (
                "static/beskid_runtime.lib",
                "shared/beskid_runtime.dll",
                Some("shared/beskid_runtime_import.lib"),
            ),
            TargetTriple::Other(_) => unreachable!("target validation rejects unsupported triples"),
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
    UnversionedExportSymbol(String),
}
