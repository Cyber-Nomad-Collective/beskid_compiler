use super::super::{ABI_V5, AbiManifestV5, ManifestValidationError, TargetMetadata, TrapCode};
use super::identity::{TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS, canonical_runtime_package};
use super::source_contract::{
    SourceContract, source_assembly, source_function, source_intrinsic, source_layout, source_platform_import,
};

impl AbiManifestV5 {
    pub fn canonical_runtime(target: TargetMetadata) -> Self {
        let source: SourceContract = serde_json::from_str(crate::generated::abi_v5_contract::ABI_V5_SOURCE_JSON)
            .expect("build-validated ABI-v5 generated source");
        let _target_source = source
            .targets
            .iter()
            .find(|entry| entry.triple == target.triple.as_str())
            .expect("target validation and generated source agree");
        let target_slug = target.triple.as_str();
        Self {
            abi_version: ABI_V5,
            trap_exit_status: TRAP_EXIT_STATUS,
            trap_diagnostic: TRAP_DIAGNOSTIC_PREFIX.into(),
            imports: Vec::new(),
            exports: source.exports.iter().map(source_function).collect(),
            layouts: source
                .layouts
                .iter()
                .filter(|layout| layout.target.as_deref().is_none_or(|value| value == target_slug))
                .map(source_layout)
                .collect(),
            trusted_runtime_package: Some(canonical_runtime_package()),
            trusted_runtime_intrinsics: source.intrinsics.iter().map(source_intrinsic).collect(),
            platform_imports: source
                .platform_imports
                .iter()
                .filter(|entry| entry.target == target_slug)
                .map(source_platform_import)
                .collect(),
            assembly_exports: source.assembly.iter().map(|entry| source_assembly(entry, target_slug)).collect(),
            traps: crate::generated::abi_v5_contract::ABI_V5_TRAPS
                .iter()
                .map(|(_, code)| TrapCode::try_from(*code).expect("validated trap code"))
                .collect(),
            target,
        }
    }

    pub(in crate::abi_v5) fn validate_canonical_bootstrap_contract(&self) -> Result<(), ManifestValidationError> {
        if !self.imports.is_empty() {
            return Err(ManifestValidationError::InvalidRuntimeImportSet { actual: self.imports.clone() });
        }
        let canonical = Self::canonical_runtime(self.target.clone());
        if self.exports != canonical.exports {
            return Err(ManifestValidationError::InvalidRuntimeExportSet { actual: self.exports.clone() });
        }
        if self.trusted_runtime_intrinsics != canonical.trusted_runtime_intrinsics {
            return Err(ManifestValidationError::InvalidRuntimeIntrinsicSet {
                actual: self.trusted_runtime_intrinsics.clone(),
            });
        }
        if self.platform_imports != canonical.platform_imports {
            return Err(ManifestValidationError::InvalidPlatformImportSet { actual: self.platform_imports.clone() });
        }
        if self.layouts != canonical.layouts {
            return Err(ManifestValidationError::InvalidRuntimeLayoutSet { actual: self.layouts.clone() });
        }
        Ok(())
    }
}
