//! Resolve the exact validated ABI-v5 runtime kit shipped with the toolchain.

use std::path::{Path, PathBuf};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_abi::runtime_source::resolve_canonical_runtime_kit;

use crate::api::{BuildProfile, RuntimeLinkProfile, RuntimeStrategy};
use crate::error::{AotError, AotResult};
use crate::target::detect_target;

const ENV_RUNTIME_PREFIX: &str = "BESKID_RUNTIME_PREFIX";

/// Default AOT runtime strategy: one exact installed ABI-v5 kit.
pub fn default_runtime_strategy(
    profile: BuildProfile,
    target_triple: Option<&str>,
    link_profile: RuntimeLinkProfile,
) -> AotResult<RuntimeStrategy> {
    let prefix = runtime_prefix()?;
    installed_runtime_strategy(&prefix, profile, target_triple, link_profile)
}

/// Construct an exact installed-kit request without probing alternate archives or profiles.
pub fn installed_runtime_strategy(
    prefix: &Path,
    profile: BuildProfile,
    target_triple: Option<&str>,
    link_profile: RuntimeLinkProfile,
) -> AotResult<RuntimeStrategy> {
    require_single_runtime_profile(link_profile)?;
    Ok(RuntimeStrategy::UseInstalledKit {
        prefix: prefix.to_path_buf(),
        target: runtime_target(target_triple)?,
        profile: kit_profile(profile),
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

/// Resolve the static artifact from the toolchain's exact ABI-v5 kit.
pub fn resolve_bundled_runtime_archive(
    profile: BuildProfile,
    target_triple: Option<&str>,
    link_profile: RuntimeLinkProfile,
) -> AotResult<PathBuf> {
    resolve_installed_runtime_archive(&runtime_prefix()?, profile, target_triple, link_profile)
}

/// Resolve one exact prefix/target/profile and return its hash-verified static artifact.
pub fn resolve_installed_runtime_archive(
    prefix: &Path,
    profile: BuildProfile,
    target_triple: Option<&str>,
    link_profile: RuntimeLinkProfile,
) -> AotResult<PathBuf> {
    require_single_runtime_profile(link_profile)?;
    let target = runtime_target(target_triple)?;
    let kit = resolve_aot_runtime_kit(prefix, &target, kit_profile(profile))?;
    Ok(kit.static_library)
}

pub(crate) fn resolve_aot_runtime_kit(
    prefix: &Path,
    target: &TargetMetadata,
    profile: RuntimeKitProfile,
) -> AotResult<beskid_abi::runtime_kit::ResolvedRuntimeKit> {
    let kit = resolve_canonical_runtime_kit(prefix, target, profile).map_err(|error| {
        AotError::RuntimeBuild {
            message: format!(
                "ABI-v5 runtime kit validation failed for `{}`: {error:?}",
                prefix.display()
            ),
        }
    })?;
    Ok(kit)
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

fn require_single_runtime_profile(link_profile: RuntimeLinkProfile) -> AotResult<()> {
    if link_profile == RuntimeLinkProfile::Minimal {
        return Err(AotError::InvalidRequest {
            message:
                "ABI v5 has one hosted runtime; the legacy minimal runtime profile is unavailable"
                    .into(),
        });
    }
    Ok(())
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

fn profile_dir_name(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Release => "release",
        BuildProfile::Debug => "debug",
    }
}

fn kit_profile(profile: BuildProfile) -> RuntimeKitProfile {
    match profile {
        BuildProfile::Debug => RuntimeKitProfile::Debug,
        BuildProfile::Release => RuntimeKitProfile::Release,
    }
}
