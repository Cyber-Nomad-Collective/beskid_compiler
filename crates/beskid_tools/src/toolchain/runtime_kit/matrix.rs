use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow, bail};
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::ResolvedRuntimeKit;
use beskid_abi::runtime_source::resolve_canonical_runtime_kit;

use super::build::build;
use super::model::{RuntimeKitBuildOptions, RuntimeKitMatrixBuildOptions, RuntimeKitProfile};
use super::verification::{RuntimeArtifactKind, verify_provenance_symbol_list};

/// Publish every requested profile for one ABI-v5 target.
///
/// A matrix must contain exactly one debug and one release artifact set.  Rejecting partial or
/// duplicate matrices keeps release automation from silently shipping a target with only the
/// profile exercised on the build host.
pub fn build_matrix(options: RuntimeKitMatrixBuildOptions) -> Result<Vec<ResolvedRuntimeKit>> {
    if options.profiles.len() != 2 {
        bail!("runtime-kit matrix must contain exactly debug and release profiles");
    }
    let mut saw_debug = false;
    let mut saw_release = false;
    // Validate every profile before publishing either one. In particular, a rejected release
    // provenance report must never leave a debug kit behind in an otherwise empty prefix.
    for artifacts in &options.profiles {
        let seen = match artifacts.profile {
            RuntimeKitProfile::Debug => &mut saw_debug,
            RuntimeKitProfile::Release => &mut saw_release,
        };
        if std::mem::replace(seen, true) {
            bail!("runtime-kit matrix contains duplicate {} profile", artifacts.profile.as_str());
        }
        verify_provenance_symbol_list(
            &options.target,
            &artifacts.static_provenance_symbol_list,
            RuntimeArtifactKind::StaticArchive,
        )?;
        verify_provenance_symbol_list(
            &options.target,
            &artifacts.shared_provenance_symbol_list,
            RuntimeArtifactKind::SharedLibrary,
        )?;
    }
    if !saw_debug || !saw_release {
        bail!("runtime-kit matrix must contain both debug and release profiles");
    }

    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == options.target)
        .ok_or_else(|| anyhow!("unsupported ABI-v5 runtime target `{}`", options.target))?;
    let final_target_root = options.prefix.join("lib/beskid-runtime/abi-5").join(target.triple.as_str());
    if final_target_root.exists() {
        bail!("runtime-kit matrix destination already exists: {}", final_target_root.display());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let staging_prefix = options.prefix.join(format!(
        ".beskid-runtime-kit-matrix-{}-{}-{nonce}",
        std::process::id(),
        target.triple.as_str()
    ));
    let profiles = options.profiles.iter().map(|artifacts| artifacts.profile).collect::<Vec<_>>();
    let publish = (|| -> Result<Vec<ResolvedRuntimeKit>> {
        for artifacts in options.profiles {
            build(RuntimeKitBuildOptions {
                prefix: staging_prefix.clone(),
                target: options.target.clone(),
                profile: artifacts.profile,
                static_library: artifacts.static_library,
                shared_library: artifacts.shared_library,
                shared_import_library: artifacts.shared_import_library,
            })?;
        }
        let staged_target_root = staging_prefix.join("lib/beskid-runtime/abi-5").join(target.triple.as_str());
        let parent = final_target_root.parent().expect("ABI-v5 runtime target root has a parent");
        std::fs::create_dir_all(parent)
            .map_err(|error| anyhow!("create ABI-v5 runtime-kit destination parent `{}`: {error}", parent.display()))?;
        std::fs::rename(&staged_target_root, &final_target_root).map_err(|error| {
            anyhow!("atomically publish ABI-v5 runtime-kit matrix `{}`: {error}", final_target_root.display())
        })?;
        profiles
            .into_iter()
            .map(|profile| {
                resolve_canonical_runtime_kit(&options.prefix, &target, profile.into())
                    .map_err(|error| anyhow!("resolve published ABI-v5 runtime-kit matrix: {error:?}"))
            })
            .collect()
    })();
    let _ = std::fs::remove_dir_all(&staging_prefix);
    publish
}
