use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// Identity of the Corelib bundle embedded in the running toolchain binary (`<version>-<fingerprint
/// prefix>`), registered once by the binary before it resolves Corelib.
static TOOLCHAIN_CORELIB_IDENTITY: OnceLock<String> = OnceLock::new();

/// Register the identity of the Corelib embedded in this toolchain. It names the per-user Corelib
/// root of this toolchain, so two toolchains outside an installed prefix never share or replace
/// each other's Corelib. Registering the same identity again is a no-op; a different one is an
/// error (one process runs one toolchain).
pub fn register_toolchain_corelib_identity(identity: &str) -> Result<(), String> {
    if identity.is_empty()
        || !identity.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'))
    {
        return Err(format!("invalid toolchain Corelib identity `{identity}`"));
    }
    let registered = TOOLCHAIN_CORELIB_IDENTITY.get_or_init(|| identity.to_owned());
    if registered != identity {
        return Err(format!("toolchain Corelib identity already registered as `{registered}`, not `{identity}`"));
    }
    Ok(())
}

/// The registered identity of this toolchain's embedded Corelib, if any.
pub fn toolchain_corelib_identity() -> Option<&'static str> {
    TOOLCHAIN_CORELIB_IDENTITY.get().map(String::as_str)
}

/// Resolve the corelib workspace root from an optional override or the installed toolchain layout.
///
/// A corelib shipped beside the executable wins. Standalone executables materialize their embedded
/// snapshot in a per-user, per-toolchain directory (`~/.beskid/corelib/<identity>`) instead, so a
/// normal installation needs no environment variables and two toolchains never share one root.
/// Without a registered identity there is no per-user root.
pub fn installed_corelib_root() -> Result<PathBuf, InstalledToolchainPrefixError> {
    if let Some(root) = non_empty_environment_path(ENV_CORELIB_ROOT) {
        return Ok(root);
    }

    let user_root = user_home_directory()
        .zip(toolchain_corelib_identity())
        .map(|(home, identity)| user_corelib_root(&home, identity));
    match std::env::current_exe() {
        Ok(executable) => installed_corelib_root_for_executable(&executable, user_root.as_deref()),
        Err(_) if user_root.is_some() => Ok(user_root.expect("user root checked above")),
        Err(error) => Err(InstalledToolchainPrefixError::CurrentExe(error)),
    }
}

/// Resolve corelib for a known executable, preferring a bundled workspace at `<prefix>/beskid_corelib`
/// over this toolchain's per-user root `user_root` (see [`user_corelib_root`]).
pub fn installed_corelib_root_for_executable(
    executable: &Path,
    user_root: Option<&Path>,
) -> Result<PathBuf, InstalledToolchainPrefixError> {
    match installed_toolchain_prefix_for_executable(executable) {
        Ok(prefix) => {
            let bundled = prefix.join("beskid_corelib");
            if bundled.is_dir() || user_root.is_none() {
                return Ok(bundled);
            }
        }
        Err(error) if user_root.is_none() => return Err(error),
        Err(_) => {}
    }

    Ok(user_root.expect("missing user root returned above").to_path_buf())
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

/// The per-user Corelib root of the toolchain whose embedded Corelib has `identity`.
pub fn user_corelib_root(home: &Path, identity: &str) -> PathBuf {
    home.join(".beskid").join("corelib").join(identity)
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
