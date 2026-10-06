//! ABI-v5 runtime manifest loading and artifact generation.

mod analysis_codegen;
mod v5;

pub use v5::{
    CHECKED_RUNTIME_CLONES, CheckedRuntimeClone, GeneratedV5Artifacts, RuntimeManifestV5, checked_runtime_clone,
    dynamic_v1_contract, generate_v5_artifacts, glue_owner_v1_contract, independently_versioned_runtime_contract,
    load_v5_manifest_source, runtime_layout_source, write_v5_artifacts,
};

use std::path::{Path, PathBuf};

/// Generate the analysis builtin table from the normative ABI-v5 manifest.
pub fn generate_analysis_with_v5_intrinsics_from_source(
    source: &str,
    base: &str,
    out_path: &Path,
) -> Result<(), String> {
    let runtime = load_v5_manifest_source(source)?;
    let generated = analysis_codegen::append_analysis_intrinsics(base, &runtime)?;
    match std::fs::read(out_path) {
        Ok(existing) if existing == generated.as_bytes() => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(out_path, generated).map_err(|err| err.to_string())
}

pub fn default_manifest_path(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("../../runtime_manifest.bsol")
}
