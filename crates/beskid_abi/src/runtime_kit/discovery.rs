use std::path::{Path, PathBuf};

use crate::abi_v5::TargetMetadata;

use super::paths::{ENV_CORELIB_ROOT, ENV_RUNTIME_PREFIX};

#[derive(Debug)]
pub enum InstalledToolchainPrefixError {
    CurrentExe(std::io::Error),
    MissingParent { executable: PathBuf },
    InvalidBinLayout { executable: PathBuf },
    MissingInstallPrefix { executable: PathBuf },
}

impl std::fmt::Display for InstalledToolchainPrefixError {
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

impl std::error::Error for InstalledToolchainPrefixError {}

/// Resolve the installed toolchain prefix from the process executable.
pub fn installed_toolchain_prefix() -> Result<PathBuf, InstalledToolchainPrefixError> {
    let executable = std::env::current_exe().map_err(InstalledToolchainPrefixError::CurrentExe)?;
    installed_toolchain_prefix_for_executable(&executable)
}

/// Resolve the runtime-kit prefix from an optional override or the installed toolchain layout.
pub fn installed_runtime_prefix() -> Result<PathBuf, InstalledToolchainPrefixError> {
    if let Some(prefix) = non_empty_environment_path(ENV_RUNTIME_PREFIX) {
        return Ok(prefix);
    }
    installed_toolchain_prefix()
}

/// Resolve the corelib workspace root from an optional override or the installed toolchain layout.
///
/// A corelib shipped beside the executable wins. Standalone executables materialize their embedded
/// snapshot in the per-user Beskid directory instead, so a normal installation needs no environment
/// variables.
pub fn installed_corelib_root() -> Result<PathBuf, InstalledToolchainPrefixError> {
    if let Some(root) = non_empty_environment_path(ENV_CORELIB_ROOT) {
        return Ok(root);
    }

    let home = user_home_directory();
    match std::env::current_exe() {
        Ok(executable) => installed_corelib_root_for_executable(&executable, home.as_deref()),
        Err(_) if home.is_some() => Ok(user_corelib_root(home.as_deref().expect("home checked above"))),
        Err(error) => Err(InstalledToolchainPrefixError::CurrentExe(error)),
    }
}

/// Resolve corelib for a known executable, preferring a bundled workspace at `<prefix>/beskid_corelib`.
pub fn installed_corelib_root_for_executable(
    executable: &Path,
    home: Option<&Path>,
) -> Result<PathBuf, InstalledToolchainPrefixError> {
    match installed_toolchain_prefix_for_executable(executable) {
        Ok(prefix) => {
            let bundled = prefix.join("beskid_corelib");
            if bundled.is_dir() || home.is_none() {
                return Ok(bundled);
            }
        }
        Err(error) if home.is_none() => return Err(error),
        Err(_) => {}
    }

    Ok(user_corelib_root(home.expect("missing home returned above")))
}

fn non_empty_environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn user_home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn user_corelib_root(home: &Path) -> PathBuf {
    home.join(".beskid").join("beskid_corelib")
}

/// Derive the install prefix for a known executable path (`<prefix>/bin/<tool>`).
pub fn installed_toolchain_prefix_for_executable(executable: &Path) -> Result<PathBuf, InstalledToolchainPrefixError> {
    let bin = executable
        .parent()
        .ok_or_else(|| InstalledToolchainPrefixError::MissingParent { executable: executable.to_path_buf() })?;
    if bin.file_name().is_none_or(|name| name != "bin") {
        return Err(InstalledToolchainPrefixError::InvalidBinLayout { executable: executable.to_path_buf() });
    }
    bin.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| InstalledToolchainPrefixError::MissingInstallPrefix { executable: executable.to_path_buf() })
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
    TargetMetadata::for_triple(triple).map_err(|_| HostRuntimeTargetError::UnsupportedTarget { triple: triple.into() })
}
