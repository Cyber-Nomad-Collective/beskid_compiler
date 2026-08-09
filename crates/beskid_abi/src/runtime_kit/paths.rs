use std::path::{Path, PathBuf};

use crate::abi_v5::TargetMetadata;

use super::model::BuildProfile;

pub const RUNTIME_KIT_SCHEMA_VERSION: u32 = 1;

/// Process environment override for the exact installed toolchain prefix.
///
/// When unset, consumers derive the prefix from the current executable
/// (`<prefix>/bin/<tool>` → `<prefix>`). There is no search-path or nearest-kit fallback.
pub const ENV_RUNTIME_PREFIX: &str = "BESKID_RUNTIME_PREFIX";

pub(super) const INSTALLED_RUNTIME_ROOT: &str = "lib/beskid-runtime/abi-5";

/// Relative installed root shared by every ABI-v5 consumer (`lib/beskid-runtime/abi-5`).
pub fn installed_runtime_root() -> &'static str {
    INSTALLED_RUNTIME_ROOT
}

/// Exact profile directory name under the target kit root.
pub fn profile_directory_name(profile: BuildProfile) -> &'static str {
    profile_directory(profile)
}

/// Path to `abi.json` for one exact prefix/target/profile coordinate.
pub fn exact_kit_metadata_path(prefix: &Path, target: &TargetMetadata, profile: BuildProfile) -> PathBuf {
    prefix.join(INSTALLED_RUNTIME_ROOT).join(target.triple.as_str()).join(profile_directory(profile)).join("abi.json")
}

pub(super) fn profile_directory(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    }
}

pub(super) fn artifact_paths_for_target(target: &TargetMetadata) -> (&'static str, &'static str, Option<&'static str>) {
    match target.object_format.as_str() {
        "elf" => ("static/libbeskid_runtime.a", "shared/libbeskid_runtime.so", None),
        "macho" => ("static/libbeskid_runtime.a", "shared/libbeskid_runtime.dylib", None),
        "coff" => ("static/beskid_runtime.lib", "shared/beskid_runtime.dll", Some("shared/beskid_runtime_import.lib")),
        _ => unreachable!("target validation rejects unsupported object formats"),
    }
}
