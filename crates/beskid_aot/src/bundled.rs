//! Resolve prebuilt Beskid runtime static libraries shipped with the toolchain.

use std::path::{Path, PathBuf};

use beskid_abi::BESKID_RUNTIME_ABI_VERSION;

use crate::api::{BuildProfile, RuntimeStrategy};
use crate::error::{AotError, AotResult};
use crate::target::detect_target;

const ENV_RUNTIME_ARCHIVE: &str = "BESKID_RUNTIME_ARCHIVE";

/// Default AOT runtime strategy: bundled prebuilt archive for the host target and profile.
pub fn default_runtime_strategy(
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<RuntimeStrategy> {
    let path = resolve_bundled_runtime_archive(profile, target_triple)?;
    Ok(RuntimeStrategy::UsePrebuilt {
        path,
        abi_version: BESKID_RUNTIME_ABI_VERSION,
    })
}

/// Locate a prebuilt `beskid_runtime_bridge` static library.
pub fn resolve_bundled_runtime_archive(
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<PathBuf> {
    if let Ok(path) = std::env::var(ENV_RUNTIME_ARCHIVE) {
        let path = PathBuf::from(path.trim());
        if path.is_file() {
            return Ok(path);
        }
        return Err(AotError::RuntimeArchiveMissing { path });
    }

    if let Some(path) = install_layout_runtime_archive(profile, target_triple)? {
        return Ok(path);
    }

    if let Some(path) = workspace_dev_runtime_archive(profile) {
        return Ok(path);
    }

    let target = detect_target(target_triple)?;
    Err(AotError::RuntimeBuild {
        message: format!(
            "prebuilt Beskid runtime archive not found for target `{}` and profile `{:?}`. \
             Install a Beskid toolchain that includes `lib/beskid-runtime/abi-{BESKID_RUNTIME_ABI_VERSION}/`, \
             set `{ENV_RUNTIME_ARCHIVE}` to a `libbeskid_runtime_bridge` static library, \
             or run `cargo build -p beskid_runtime_bridge` in the compiler workspace.",
            target.triple, profile
        ),
    })
}

fn install_layout_runtime_archive(
    profile: BuildProfile,
    target_triple: Option<&str>,
) -> AotResult<Option<PathBuf>> {
    let Ok(exe) = std::env::current_exe() else {
        return Ok(None);
    };
    let target = detect_target(target_triple)?;
    let lib_name = runtime_bridge_library_name(target.static_lib_ext);
    let profile_dir = profile_dir_name(profile);

    for ancestor in exe.ancestors() {
        let candidate = ancestor
            .join("lib")
            .join("beskid-runtime")
            .join(format!("abi-{BESKID_RUNTIME_ABI_VERSION}"))
            .join(&target.triple)
            .join(profile_dir)
            .join(lib_name);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

fn workspace_dev_runtime_archive(profile: BuildProfile) -> Option<PathBuf> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib_name = if cfg!(target_os = "windows") {
        "beskid_runtime_bridge.lib"
    } else {
        "libbeskid_runtime_bridge.a"
    };
    let candidate = workspace
        .join("target")
        .join(profile_dir_name(profile))
        .join(lib_name);
    candidate.is_file().then_some(candidate)
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
