//! Resolve the exact validated ABI-v5 runtime kit shipped with the toolchain.

use std::path::{Path, PathBuf};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_abi::runtime_source::resolve_canonical_runtime_kit;

use crate::api::{BuildProfile, RuntimeKitRequest};
use crate::error::{AotError, AotResult};

const ENV_RUNTIME_PREFIX: &str = "BESKID_RUNTIME_PREFIX";

/// Default AOT runtime request: one exact installed ABI-v5 kit.
pub fn default_runtime_strategy(
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<RuntimeKitRequest> {
    let prefix = runtime_prefix()?;
    installed_runtime_strategy(&prefix, profile, target_triple)
}

/// Construct an exact installed-kit request without probing alternate archives or profiles.
pub fn installed_runtime_strategy(
    prefix: &Path,
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<RuntimeKitRequest> {
    Ok(RuntimeKitRequest {
        prefix: prefix.to_path_buf(),
        target: runtime_target(target_triple)?,
        profile: kit_profile(profile),
    })
}

/// Resolve one exact prefix/target/profile and return its hash-verified static artifact.
pub fn resolve_installed_runtime_archive(
    prefix: &Path,
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<PathBuf> {
    let target = runtime_target(target_triple)?;
    let kit = resolve_aot_runtime_kit(prefix, &target, kit_profile(profile))?;
    Ok(kit.static_library)
}

pub(crate) fn resolve_aot_runtime_kit(
    prefix: &Path,
    target: &TargetMetadata,
    profile: RuntimeKitProfile,
) -> AotResult<beskid_abi::runtime_kit::ResolvedRuntimeKit> {
    resolve_canonical_runtime_kit(prefix, target, profile).map_err(|error| AotError::RuntimeBuild {
        message: format!(
            "ABI-v5 runtime kit validation failed for `{}`: {error:?}",
            prefix.display()
        ),
    })
}

fn runtime_prefix() -> AotResult<PathBuf> {
    if let Some(prefix) = std::env::var_os(ENV_RUNTIME_PREFIX) {
        return Ok(PathBuf::from(prefix));
    }
    let executable = std::env::current_exe().map_err(|error| AotError::RuntimeBuild {
        message: format!("cannot locate current executable for ABI-v5 runtime prefix: {error}"),
    })?;
    let bin = executable.parent().ok_or_else(|| AotError::RuntimeBuild {
        message: format!(
            "current executable has no parent: `{}`",
            executable.display()
        ),
    })?;
    bin.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| AotError::RuntimeBuild {
            message: format!(
                "current executable has no install prefix: `{}`",
                executable.display()
            ),
        })
}

fn runtime_target(target_triple: Option<&str>) -> AotResult<TargetMetadata> {
    let triple = target_triple.map_or_else(host_runtime_triple, str::to_owned);
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == triple)
        .ok_or_else(|| AotError::RuntimeBuild {
            message: format!("unsupported ABI-v5 runtime target `{triple}`"),
        })
}

fn host_runtime_triple() -> String {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        (arch, os) => return format!("{arch}-unsupported-{os}"),
    }
    .into()
}

fn kit_profile(profile: BuildProfile) -> RuntimeKitProfile {
    match profile {
        BuildProfile::Debug => RuntimeKitProfile::Debug,
        BuildProfile::Release => RuntimeKitProfile::Release,
    }
}
