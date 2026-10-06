//! Runtime admission consumes immutable compiler-build identity, never a mutable sidecar.
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
include!(concat!(env!("OUT_DIR"), "/compiler_driver_identity.rs"));

pub struct QualifiedCompilerDriver {
    path: PathBuf,
    sha256: String,
}
impl QualifiedCompilerDriver {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn verify(&self) -> Result<(), String> {
        let bytes = fs::read(&self.path).map_err(|e| e.to_string())?;
        if bytes.len() > 128 * 1024 * 1024 || format!("{:x}", Sha256::digest(bytes)) != self.sha256 {
            return Err("installed compiler driver changed".into());
        }
        Ok(())
    }
}
pub fn installed_compiler_driver() -> Result<QualifiedCompilerDriver, String> {
    let (target, digest, _source) = QUALIFIED_COMPILER_DRIVER.ok_or(
        "compiler was not built with an independently qualified native tool driver; reinstall the complete toolchain",
    )?;
    let host = crate::runtime_kit::host_runtime_triple().map_err(|e| e.to_string())?;
    if target != host {
        return Err("compiler driver build target differs from actual host".into());
    }
    let executable =
        fs::canonicalize(std::env::current_exe().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let prefix =
        crate::runtime_kit::installed_toolchain_prefix_for_executable(&executable).map_err(|e| e.to_string())?;
    let path = prefix.join("bin").join(if cfg!(windows) {
        "beskid_native_tool_driver.exe"
    } else {
        "beskid_native_tool_driver"
    });
    let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err("installed compiler driver must be a bounded regular file".into());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if canonical.parent() != executable.parent() {
        return Err("compiler driver escaped actual executable prefix".into());
    }
    let result = QualifiedCompilerDriver { path: canonical, sha256: digest.into() };
    result.verify()?;
    Ok(result)
}
