//! ABI-v5 runtime manifest loading and artifact generation.

mod analysis_codegen;
mod v5;

pub use v5::{
    GeneratedV5Artifacts, RuntimeManifestV5, generate_v5_artifacts, load_v5_manifest_source, write_v5_artifacts,
};

use std::path::{Path, PathBuf};

/// Generate the analysis builtin table from the normative ABI-v5 manifest.
pub fn generate_analysis_with_v5_intrinsics_from_source(
    source: &str,
    base: &str,
    out_path: &Path,
) -> Result<(), String> {
    let runtime = load_v5_manifest_source(source)?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(out_path, analysis_codegen::append_analysis_intrinsics(base, &runtime))
        .map_err(|err| err.to_string())
}

pub fn default_manifest_path(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("../../runtime_manifest.bsol")
}
