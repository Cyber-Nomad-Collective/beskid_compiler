use super::super::{AbiManifestV5, ManifestValidationError};

pub fn render_runtime_c_header(manifest: &AbiManifestV5) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    Ok(include_str!(concat!(env!("OUT_DIR"), "/beskid_runtime_abi_v5.h")).into())
}

pub fn render_runtime_asm_include(manifest: &AbiManifestV5) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    crate::generated::abi_v5_contract::ABI_V5_ASM_INCLUDES
        .iter()
        .find_map(|(target, source)| (*target == manifest.target.triple.as_str()).then(|| (*source).into()))
        .ok_or(ManifestValidationError::InvalidRuntimeAuditMetadata)
}
