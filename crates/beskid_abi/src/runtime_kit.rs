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
    host_runtime_target, host_runtime_triple, installed_runtime_prefix, installed_runtime_prefix_for_executable,
    HostRuntimeTargetError, InstalledRuntimePrefixError,
};
pub use model::{
    BuildProfile, InvalidBuildProfile, ResolvedRuntimeKit, RuntimeArtifact, RuntimeArtifacts, RuntimeKitBuildError,
    RuntimeKitBuildRequest, RuntimeKitMetadata, RuntimeKitResolutionError, RuntimeKitValidationError,
};
pub use paths::{
    exact_kit_metadata_path, installed_runtime_root, profile_directory_name, ENV_RUNTIME_PREFIX,
    RUNTIME_KIT_SCHEMA_VERSION,
};
pub use resolution::resolve_installed_runtime_kit;
