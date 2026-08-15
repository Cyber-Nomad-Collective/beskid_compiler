use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use beskid_abi::runtime_kit::ResolvedRuntimeKit;

use super::build::build;
use super::model::{RuntimeKitBuildOptions, RuntimeKitProfile};

// `build_native_host` can be called concurrently by integration tests in one process. Pair the
// clock nonce with a monotonic sequence so every invocation owns its staging directory even when
// the system clock resolution is coarser than the calls.
static NATIVE_HOST_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Build and atomically publish the compiler-owned canonical runtime for this native host.
/// The caller supplies only its empty destination and profile; runtime source and native library
/// paths are constructed here, so a bridge or arbitrary archive cannot enter the ABI-v5 layout.
pub(crate) fn aot_profile(profile: RuntimeKitProfile) -> beskid_aot::BuildProfile {
    match profile {
        RuntimeKitProfile::Debug => beskid_aot::BuildProfile::Debug,
        RuntimeKitProfile::Release => beskid_aot::BuildProfile::Release,
    }
}

pub fn build_native_host(prefix: PathBuf, profile: RuntimeKitProfile) -> Result<ResolvedRuntimeKit> {
    let target = beskid_abi::runtime_kit::host_runtime_target()
        .map_err(|error| anyhow!("unsupported ABI-v5 native host: {error}"))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let sequence = NATIVE_HOST_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging = std::env::temp_dir().join(format!(
        "beskid-native-runtime-{}-{}-{nonce}-{sequence}",
        std::process::id(),
        profile.as_str()
    ));
    let authority = beskid_aot::require_canonical_host_emit_authority()
        .map_err(|error| anyhow!("canonical host emit authority: {error}"))?;
    let emit_profile = aot_profile(profile);
    let pair = beskid_aot::emit_host_platform_library_pair(&authority, staging.clone(), "beskid_runtime", emit_profile)
        .map_err(|error| anyhow!("link canonical native runtime: {error}"))?;
    // Windows COFF kits require the companion import library beside the shared DLL.
    // Dropping `pair.shared_import_library` made `build_native_host` publish invalid
    // Windows ABI-v5 kits (`shared_import_library: None`) even when the linker emitted it.
    let shared_import_library = pair.shared_import_library;
    if target.object_format.as_str() == "coff" {
        let import = shared_import_library.as_ref().ok_or_else(|| {
            anyhow!("Windows ABI-v5 native runtime kit for `{}` requires a COFF import library", target.triple.as_str())
        })?;
        if !import.is_file() {
            bail!("Windows ABI-v5 native runtime kit missing COFF import library at {}", import.display());
        }
    } else if shared_import_library.is_some() {
        bail!("non-COFF ABI-v5 target `{}` must not publish a shared import library", target.triple.as_str());
    }
    let result = build(RuntimeKitBuildOptions {
        prefix,
        target: target.triple.as_str().to_owned(),
        profile,
        static_library: pair.static_library,
        shared_library: pair.shared_library,
        shared_import_library,
    });
    let _ = std::fs::remove_dir_all(staging);
    result
}
