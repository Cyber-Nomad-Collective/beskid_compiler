//! Serializable metadata for installed native ABI-v5 runtime kits.

mod build;
mod discovery;
mod hashing;
mod model;
mod paths;
mod resolution;
mod validation;

pub use build::build_runtime_kit;
pub use discovery::{
    HostRuntimeTargetError, InstalledToolchainPrefixError, host_runtime_target, host_runtime_triple,
    installed_corelib_root, installed_corelib_root_for_executable, installed_runtime_prefix,
    installed_toolchain_prefix, installed_toolchain_prefix_for_executable,
};
pub use model::{
    BuildProfile, InvalidBuildProfile, ResolvedRuntimeKit, RuntimeArtifact, RuntimeArtifacts, RuntimeKitBuildError,
    RuntimeKitBuildRequest, RuntimeKitMetadata, RuntimeKitResolutionError, RuntimeKitValidationError,
};
pub use paths::{
    ENV_CORELIB_ROOT, ENV_RUNTIME_PREFIX, RUNTIME_KIT_SCHEMA_VERSION, exact_kit_metadata_path, installed_runtime_root,
    profile_directory_name,
};
pub use resolution::resolve_installed_runtime_kit;
