//! Resolve prebuilt Beskid runtime static libraries shipped with the toolchain.
//!
//! Prebuilt archives are laid out under `lib/beskid-runtime/abi-{version}/` where
//! `{version}` matches [`beskid_abi::BESKID_RUNTIME_ABI_VERSION`] (currently **4**).

use std::path::{Path, PathBuf};

use beskid_abi::BESKID_RUNTIME_ABI_VERSION;

use crate::api::{BuildProfile, RuntimeLinkProfile, RuntimeStrategy};
use crate::error::{AotError, AotResult};
use crate::target::detect_target;

const ENV_RUNTIME_ARCHIVE: &str = "BESKID_RUNTIME_ARCHIVE";

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
        RuntimeLinkProfile::Std => "cargo build -p beskid_runtime_bridge in the compiler workspace",
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
    link_profile: RuntimeLinkProfile,
) -> AotResult<Option<PathBuf>> {
    let Ok(exe) = std::env::current_exe() else {
        return Ok(None);
    };
    let target = detect_target(target_triple)?;
    let lib_name = runtime_archive_library_name(target.static_lib_ext, link_profile);
    let profile_dir = profile_dir_name(profile);
    let link_dir = runtime_link_profile_dir(link_profile);

    for ancestor in exe.ancestors() {
        let candidate = ancestor
            .join("lib")
            .join("beskid-runtime")
            .join(format!("abi-{BESKID_RUNTIME_ABI_VERSION}"))
            .join(&target.triple)
            .join(link_dir)
            .join(profile_dir)
            .join(&lib_name);
        if candidate.is_file() {
            return Ok(Some(candidate));
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

fn runtime_link_profile_dir(link_profile: RuntimeLinkProfile) -> &'static str {
    match link_profile {
        RuntimeLinkProfile::Std => "std",
        RuntimeLinkProfile::Minimal => "minimal",
    }
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
