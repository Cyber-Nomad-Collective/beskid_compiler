use std::path::Path;

use crate::runtime_kit::{
    BuildProfile, ResolvedRuntimeKit, RuntimeKitBuildError, RuntimeKitBuildRequest, RuntimeKitResolutionError,
    build_runtime_kit, resolve_installed_runtime_kit,
};

use super::sources::canonical_runtime_source_hash;

#[derive(Debug)]
pub enum CanonicalRuntimeKitError {
    Resolution(RuntimeKitResolutionError),
    SourceHashMismatch { compiler: String, kit: String },
}

#[derive(Debug)]
pub enum CanonicalRuntimeKitBuildError {
    SourceHashMismatch { compiler: String, requested: String },
    Build(RuntimeKitBuildError),
}

/// Publish a runtime kit only when it declares the exact embedded Beskid runtime corpus.
///
/// Tooling may build artifacts for any supported target/profile, but it must not publish a kit
/// under the canonical ABI-v5 identity with an unrelated source hash.
pub fn build_canonical_runtime_kit(
    request: &RuntimeKitBuildRequest,
) -> Result<ResolvedRuntimeKit, CanonicalRuntimeKitBuildError> {
    let compiler = canonical_runtime_source_hash();
    if request.runtime_source_hash != compiler {
        return Err(CanonicalRuntimeKitBuildError::SourceHashMismatch {
            compiler,
            requested: request.runtime_source_hash.clone(),
        });
    }
    build_runtime_kit(request).map_err(CanonicalRuntimeKitBuildError::Build)
}

/// Resolve a validated installed kit whose source corpus exactly matches this compiler.
pub fn resolve_canonical_runtime_kit(
    prefix: &Path,
    target: &crate::abi_v5::TargetMetadata,
    profile: BuildProfile,
) -> Result<ResolvedRuntimeKit, CanonicalRuntimeKitError> {
    let kit = resolve_installed_runtime_kit(prefix, target, profile).map_err(CanonicalRuntimeKitError::Resolution)?;
    let compiler = canonical_runtime_source_hash();
    if kit.metadata.source_hash != compiler {
        return Err(CanonicalRuntimeKitError::SourceHashMismatch { compiler, kit: kit.metadata.source_hash });
    }
    Ok(kit)
}
