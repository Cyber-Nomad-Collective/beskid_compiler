//! Resolve prebuilt Beskid runtime static libraries shipped with the toolchain.
//!
//! Installed archives are resolved through validated ABI-v5 runtime-kit metadata.

use std::path::{Path, PathBuf};

use beskid_abi::BESKID_RUNTIME_ABI_VERSION;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{
    BuildProfile as RuntimeKitProfile, resolve_installed_runtime_kit,
};

use crate::api::{BuildProfile, RuntimeLinkProfile, RuntimeStrategy};
use crate::error::{AotError, AotResult};
use crate::target::detect_target;

const ENV_RUNTIME_ARCHIVE: &str = "BESKID_RUNTIME_ARCHIVE";

/// Resolve the static archive from one exact, metadata-validated ABI-v5 runtime kit.
pub fn resolve_runtime_kit_archive_at_prefix(
    prefix: &Path,
    profile: BuildProfile,
    target_triple: &str,
) -> AotResult<PathBuf> {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == target_triple)
        .ok_or_else(|| AotError::RuntimeBuild {
            message: format!("unsupported ABI-v5 runtime kit target `{target_triple}`"),
        })?;
    let kit_profile = match profile {
        BuildProfile::Debug => RuntimeKitProfile::Debug,
        BuildProfile::Release => RuntimeKitProfile::Release,
    };
    resolve_installed_runtime_kit(prefix, &target, kit_profile)
        .map(|kit| kit.static_library)
        .map_err(|error| AotError::RuntimeBuild {
            message: format!(
                "failed to resolve exact ABI-v5 runtime kit for `{target_triple}`: {error:?}"
            ),
        })
}

/// Default AOT runtime strategy: bundled prebuilt archive for the host target and profile.
pub fn default_runtime_strategy(
    profile: BuildProfile,
    target_triple: Option<&str>,
    link_profile: RuntimeLinkProfile,
) -> AotResult<RuntimeStrategy> {
    let path = resolve_bundled_runtime_archive(profile, target_triple, link_profile)?;
    Ok(RuntimeStrategy::UsePrebuilt {
        path,
        abi_version: BESKID_RUNTIME_ABI_VERSION,
    })
}

/// Locate a prebuilt `beskid_host` static library.
///
/// Host handlers ship inside the std [`RuntimeLinkProfile::Std`] runtime bridge archive.
/// A standalone host archive remains available for toolchain packaging and diagnostics.
pub fn resolve_bundled_host_archive(
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<PathBuf> {
    if let Some(path) = workspace_dev_host_archive(profile) {
        return Ok(path);
    }

    let target = detect_target(target_triple)?;
    Err(AotError::RuntimeBuild {
        message: format!(
            "prebuilt Beskid host archive not found for target `{}` and profile `{:?}`. \
             Run `cargo build -p beskid_host` in the compiler workspace.",
            target.triple, profile
        ),
    })
}

fn workspace_dev_host_archive(profile: BuildProfile) -> Option<PathBuf> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib_name = if cfg!(target_os = "windows") {
        "beskid_host.lib"
    } else {
        "libbeskid_host.a"
    };
    let profile_dir = profile_dir_name(profile);
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));

    let mut candidates = vec![
        target_root.join(profile_dir).join(lib_name),
        target_root.join(lib_name),
    ];
    if let Ok(host_triple) = std::env::var("HOST") {
        candidates.push(
            target_root
                .join(host_triple)
                .join(profile_dir)
                .join(lib_name),
        );
    }
    if let Ok(build_target) = std::env::var("CARGO_BUILD_TARGET") {
        candidates.push(
            target_root
                .join(build_target)
                .join(profile_dir)
                .join(lib_name),
        );
    }

    candidates.into_iter().find(|path| path.is_file())
}

/// Locate a prebuilt `beskid_runtime_bridge` static library.
pub fn resolve_bundled_runtime_archive(
    profile: BuildProfile,
    target_triple: Option<&str>,
    link_profile: RuntimeLinkProfile,
) -> AotResult<PathBuf> {
    if let Ok(path) = std::env::var(ENV_RUNTIME_ARCHIVE) {
        let path = PathBuf::from(path.trim());
        if path.is_file() {
            return Ok(path);
        }
        return Err(AotError::RuntimeArchiveMissing { path });
    }

    if let Some(path) = install_layout_runtime_archive(profile, target_triple, link_profile)? {
        return Ok(path);
    }

    if let Some(path) = workspace_dev_runtime_archive(profile, link_profile) {
        return Ok(path);
    }

    let target = detect_target(target_triple)?;
    let archive_hint = runtime_archive_build_hint(link_profile);
    Err(AotError::RuntimeBuild {
        message: format!(
            "prebuilt Beskid runtime archive not found for target `{}`, profile `{:?}`, and link profile `{link_profile:?}`. \
             Install a Beskid toolchain that includes `lib/beskid-runtime/abi-{BESKID_RUNTIME_ABI_VERSION}/`, \
             set `{ENV_RUNTIME_ARCHIVE}` to a runtime bridge static library, \
             or run `{archive_hint}`.",
            target.triple, profile
        ),
    })
}

fn runtime_archive_build_hint(link_profile: RuntimeLinkProfile) -> &'static str {
    match link_profile {
        RuntimeLinkProfile::Std => {
            "cargo build -p beskid_runtime_bridge in the compiler workspace"
        }
        RuntimeLinkProfile::Minimal => {
            "cargo build -p beskid_runtime_bridge --no-default-features && \
             cp target/debug/libbeskid_runtime_bridge.a target/debug/libbeskid_runtime_minimal.a && \
             cargo build -p beskid_runtime_bridge"
        }
    }
}

fn install_layout_runtime_archive(
    profile: BuildProfile,
    target_triple: Option<&str>,
    _link_profile: RuntimeLinkProfile,
) -> AotResult<Option<PathBuf>> {
    let Ok(exe) = std::env::current_exe() else {
        return Ok(None);
    };
    let target = detect_target(target_triple)?;

    for ancestor in exe.ancestors() {
        if let Ok(archive) =
            resolve_runtime_kit_archive_at_prefix(ancestor, profile, &target.triple)
        {
            return Ok(Some(archive));
        }
    }

    Ok(None)
}

fn workspace_dev_runtime_archive(
    profile: BuildProfile,
    link_profile: RuntimeLinkProfile,
) -> Option<PathBuf> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib_name = runtime_archive_library_name(
        if cfg!(target_os = "windows") {
            "lib"
        } else {
            "a"
        },
        link_profile,
    );
    let profile_dir = profile_dir_name(profile);
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));

    let mut candidates = vec![target_root.join(profile_dir).join(&lib_name)];
    if let Ok(host_triple) = std::env::var("HOST") {
        candidates.push(
            target_root
                .join(&host_triple)
                .join(profile_dir)
                .join(&lib_name),
        );
    }
    if let Ok(build_target) = std::env::var("CARGO_BUILD_TARGET") {
        candidates.push(
            target_root
                .join(build_target)
                .join(profile_dir)
                .join(&lib_name),
        );
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn runtime_archive_library_name(static_lib_ext: &str, link_profile: RuntimeLinkProfile) -> String {
    match link_profile {
        RuntimeLinkProfile::Std => runtime_bridge_library_name(static_lib_ext).to_owned(),
        RuntimeLinkProfile::Minimal => {
            if static_lib_ext == "lib" {
                "beskid_runtime_minimal.lib".to_owned()
            } else {
                "libbeskid_runtime_minimal.a".to_owned()
            }
        }
    }
}

fn profile_dir_name(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Release => "release",
        BuildProfile::Debug => "debug",
    }
}

fn runtime_bridge_library_name(static_lib_ext: &str) -> &'static str {
    if static_lib_ext == "lib" {
        "beskid_runtime_bridge.lib"
    } else {
        "libbeskid_runtime_bridge.a"
    }
}
