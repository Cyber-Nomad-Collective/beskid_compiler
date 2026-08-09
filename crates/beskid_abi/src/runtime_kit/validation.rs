use std::collections::HashSet;

use crate::abi_v5::{ABI_V5, AbiManifestV5};

use super::model::{RuntimeKitMetadata, RuntimeKitValidationError};
use super::paths::{RUNTIME_KIT_SCHEMA_VERSION, artifact_paths_for_target};

impl RuntimeKitMetadata {
    pub fn canonical_abi_json(&self) -> Result<String, RuntimeKitValidationError> {
        self.validate()?;
        let mut output =
            serde_json::to_string_pretty(self).map_err(|_| RuntimeKitValidationError::InvalidAbiContract)?;
        output.push('\n');
        Ok(output)
    }

    pub fn validate(&self) -> Result<(), RuntimeKitValidationError> {
        if self.schema_version != RUNTIME_KIT_SCHEMA_VERSION {
            return Err(RuntimeKitValidationError::WrongSchemaVersion(self.schema_version));
        }
        if self.abi_version != ABI_V5 {
            return Err(RuntimeKitValidationError::WrongAbiVersion(self.abi_version));
        }
        self.target.validate().map_err(RuntimeKitValidationError::InvalidTarget)?;
        self.abi_contract.validate().map_err(|_| RuntimeKitValidationError::InvalidAbiContract)?;
        if self.abi_contract.target != self.target || self.abi_contract.abi_version != self.abi_version {
            return Err(RuntimeKitValidationError::ContractTargetMismatch);
        }
        if self.abi_contract != AbiManifestV5::canonical_runtime(self.target.clone()) {
            return Err(RuntimeKitValidationError::InvalidAbiContract);
        }
        for (name, hash) in [("layout_hash", &self.layout_hash), ("source_hash", &self.source_hash)] {
            validate_sha256(name, hash)?;
        }

        for artifact in self.artifacts.iter() {
            if !is_portable_relative_path(&artifact.relative_path) {
                return Err(RuntimeKitValidationError::InvalidArtifactPath(artifact.relative_path.clone()));
            }
            validate_sha256("artifact.sha256", &artifact.sha256)?;
        }

        let (static_path, shared_path, import_path) = artifact_paths_for_target(&self.target);
        let actual_import_path =
            self.artifacts.shared_import_library.as_ref().map(|artifact| artifact.relative_path.as_str());
        if self.artifacts.static_library.relative_path != static_path
            || self.artifacts.shared_library.relative_path != shared_path
            || actual_import_path != import_path
        {
            return Err(RuntimeKitValidationError::InvalidArtifactSet { target: self.target.triple.as_str().into() });
        }

        validate_allowlist(&self.import_allowlist)?;
        validate_allowlist(&self.export_allowlist)?;
        validate_allowlist(&self.loader_required_exports)?;
        if self.layout_hash != self.abi_contract.layout_hash() || self.layout_hash != self.audit.layout_hash {
            return Err(RuntimeKitValidationError::ContractLayoutHashMismatch { actual: self.layout_hash.clone() });
        }
        if self.source_hash != self.audit.runtime_source_hash {
            return Err(RuntimeKitValidationError::ContractSourceHashMismatch { actual: self.source_hash.clone() });
        }
        self.audit
            .validate(&self.abi_contract)
            .map_err(|_| RuntimeKitValidationError::ContractAuditMismatch { field: "audit".into() })?;
        if self.import_allowlist != self.audit.allowed_imports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch { field: "import_allowlist".into() });
        }
        if self.export_allowlist != self.audit.allowed_exports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch { field: "export_allowlist".into() });
        }
        if self.loader_required_exports != self.audit.loader_required_exports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch { field: "loader_required_exports".into() });
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
    value.split('/').all(|component| !component.is_empty() && component != "." && component != "..")
}

pub(super) fn validate_sha256(name: &str, value: &str) -> Result<(), RuntimeKitValidationError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(RuntimeKitValidationError::InvalidSha256 { field: name.into() });
    }
    Ok(())
}

fn validate_allowlist(symbols: &[String]) -> Result<(), RuntimeKitValidationError> {
    let mut seen = HashSet::new();
    for symbol in symbols {
        if symbol.is_empty() || !seen.insert(symbol.as_str()) {
            return Err(RuntimeKitValidationError::DuplicateAllowlistSymbol { symbol: symbol.clone() });
        }
    }
    Ok(())
}
