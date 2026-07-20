//! Resolve the exact validated ABI-v5 runtime kit shipped with the toolchain.

use std::path::{Path, PathBuf};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_abi::runtime_source::resolve_canonical_runtime_kit;

use crate::api::{BuildProfile, RuntimeKitRequest};
use crate::error::{AotError, AotResult};

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
    beskid_abi::runtime_kit::installed_runtime_prefix().map_err(|error| AotError::RuntimeBuild {
        message: error.to_string(),
    })
}

fn runtime_target(target_triple: Option<&str>) -> AotResult<TargetMetadata> {
    match target_triple {
        Some(triple) => TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == triple)
            .ok_or_else(|| AotError::RuntimeBuild {
                message: format!("unsupported ABI-v5 runtime target `{triple}`"),
            }),
        None => beskid_abi::runtime_kit::host_runtime_target().map_err(|error| {
            AotError::RuntimeBuild {
                message: error.to_string(),
            }
        }),
    }
}

fn kit_profile(profile: BuildProfile) -> RuntimeKitProfile {
    match profile {
        BuildProfile::Debug => RuntimeKitProfile::Debug,
        BuildProfile::Release => RuntimeKitProfile::Release,
    }
}
