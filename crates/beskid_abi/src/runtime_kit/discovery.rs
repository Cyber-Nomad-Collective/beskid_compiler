use std::path::{Path, PathBuf};

use crate::abi_v5::TargetMetadata;

use super::paths::ENV_RUNTIME_PREFIX;

#[derive(Debug)]
pub enum InstalledRuntimePrefixError {
    CurrentExe(std::io::Error),
    MissingParent { executable: PathBuf },
    InvalidBinLayout { executable: PathBuf },
    MissingInstallPrefix { executable: PathBuf },
}

impl std::fmt::Display for InstalledRuntimePrefixError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentExe(error) => {
                write!(formatter, "cannot locate current executable for ABI-v5 runtime prefix: {error}")
            }
            Self::MissingParent { executable } => {
                write!(formatter, "current executable has no parent: `{}`", executable.display())
            }
            Self::InvalidBinLayout { executable } => {
                write!(
                    formatter,
                    "current executable is not installed under `<prefix>/bin`: `{}`",
                    executable.display()
                )
            }
            Self::MissingInstallPrefix { executable } => {
                write!(formatter, "current executable has no install prefix: `{}`", executable.display())
            }
        }
    }
}

impl std::error::Error for InstalledRuntimePrefixError {}

/// Resolve the exact installed prefix: `BESKID_RUNTIME_PREFIX`, else parent of the executable's directory.
pub fn installed_runtime_prefix() -> Result<PathBuf, InstalledRuntimePrefixError> {
    if let Some(prefix) = std::env::var_os(ENV_RUNTIME_PREFIX) {
        return Ok(PathBuf::from(prefix));
    }
    let executable = std::env::current_exe().map_err(InstalledRuntimePrefixError::CurrentExe)?;
    installed_runtime_prefix_for_executable(&executable)
}

/// Derive the install prefix for a known executable path (`<prefix>/bin/<tool>`).
pub fn installed_runtime_prefix_for_executable(executable: &Path) -> Result<PathBuf, InstalledRuntimePrefixError> {
    let bin = executable
        .parent()
        .ok_or_else(|| InstalledRuntimePrefixError::MissingParent { executable: executable.to_path_buf() })?;
    if bin.file_name().is_none_or(|name| name != "bin") {
        return Err(InstalledRuntimePrefixError::InvalidBinLayout { executable: executable.to_path_buf() });
    }
    bin.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| InstalledRuntimePrefixError::MissingInstallPrefix { executable: executable.to_path_buf() })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRuntimeTargetError {
    UnsupportedHost { arch: String, os: String },
    UnsupportedTarget { triple: String },
}

impl std::fmt::Display for HostRuntimeTargetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedHost { arch, os } => {
                write!(formatter, "unsupported ABI-v5 runtime host `{arch}-{os}`")
            }
            Self::UnsupportedTarget { triple } => {
                write!(formatter, "unsupported ABI-v5 runtime target `{triple}`")
            }
        }
    }
}

impl std::error::Error for HostRuntimeTargetError {}

/// Triple string for the native ABI-v5 host, when the OS/arch pair is supported.
pub fn host_runtime_triple() -> Result<&'static str, HostRuntimeTargetError> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Ok("x86_64-unknown-linux-gnu"),
        ("aarch64", "macos") => Ok("aarch64-apple-darwin"),
        ("x86_64", "windows") => Ok("x86_64-pc-windows-msvc"),
        (arch, os) => Err(HostRuntimeTargetError::UnsupportedHost { arch: arch.into(), os: os.into() }),
    }
}

/// Canonical [`TargetMetadata`] for the native ABI-v5 host.
pub fn host_runtime_target() -> Result<TargetMetadata, HostRuntimeTargetError> {
    let triple = host_runtime_triple()?;
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == triple)
        .ok_or_else(|| HostRuntimeTargetError::UnsupportedTarget { triple: triple.into() })
}
