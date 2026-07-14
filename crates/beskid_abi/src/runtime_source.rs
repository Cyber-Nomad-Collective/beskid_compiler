//! Compiler-embedded authority for canonical Beskid runtime sources.
//!
//! This module grants no ambient or serializable package capability. The frontend may use the
//! returned token only for AST nodes from the exact embedded source corpus.

use std::path::Path;

use crate::abi_v5::{
    AbiManifestV5, RuntimeIntrinsic, RuntimePackageIdentity, SourceUnit, canonical_runtime_package,
    canonical_source_hash,
};
use crate::runtime_kit::{
    BuildProfile, ResolvedRuntimeKit, RuntimeKitBuildError, RuntimeKitBuildRequest,
    RuntimeKitResolutionError, build_runtime_kit, resolve_installed_runtime_kit,
};

pub const CANONICAL_BOOTSTRAP_SOURCE_PATH: &str = "src/Runtime/Bootstrap.bd";

const CANONICAL_BOOTSTRAP_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/beskid/src/Runtime/Bootstrap.bd"
));

/// The runtime source corpus built into this compiler version.
pub fn canonical_runtime_sources() -> Vec<SourceUnit> {
    vec![SourceUnit {
        logical_path: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
        source: CANONICAL_BOOTSTRAP_SOURCE.into(),
    }]
}

/// A compiler-owned proof that a node belongs to the exact canonical runtime corpus.
///
/// Deliberately has no public constructor and does not implement serialization.
#[derive(Debug)]
pub struct RuntimeIntrinsicCapability {
    source_hash: String,
    source_paths: Vec<String>,
    intrinsics: Vec<RuntimeIntrinsic>,
}

impl RuntimeIntrinsicCapability {
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }

    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.source_paths
            .iter()
            .any(|candidate| candidate == logical_path)
    }

    pub fn intrinsic_for_source(
        &self,
        logical_path: &str,
        name: &str,
    ) -> Option<&RuntimeIntrinsic> {
        if !self.authorizes_source(logical_path) {
            return None;
        }
        self.intrinsics
            .iter()
            .find(|intrinsic| intrinsic.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapabilityError {
    UnauthorizedPackage,
    SourceSetMismatch,
    InvalidManifest,
}

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

/// Hash of the corpus embedded in this compiler and eligible for ABI-v5 runtime authority.
pub fn canonical_runtime_source_hash() -> String {
    canonical_source_hash(&canonical_runtime_sources())
        .expect("embedded runtime source paths are unique")
}

/// Resolve a validated installed kit whose source corpus exactly matches this compiler.
pub fn resolve_canonical_runtime_kit(
    prefix: &Path,
    target: &crate::abi_v5::TargetMetadata,
    profile: BuildProfile,
) -> Result<ResolvedRuntimeKit, CanonicalRuntimeKitError> {
    let kit = resolve_installed_runtime_kit(prefix, target, profile)
        .map_err(CanonicalRuntimeKitError::Resolution)?;
    let compiler = canonical_runtime_source_hash();
    if kit.metadata.source_hash != compiler {
        return Err(CanonicalRuntimeKitError::SourceHashMismatch {
            compiler,
            kit: kit.metadata.source_hash,
        });
    }
    Ok(kit)
}

/// Grant trusted intrinsic selection only to the exact embedded runtime sources and manifest.
pub fn grant_runtime_intrinsics(
    package: &RuntimePackageIdentity,
    sources: &[SourceUnit],
    manifest: &AbiManifestV5,
) -> Result<RuntimeIntrinsicCapability, RuntimeCapabilityError> {
    if package != &canonical_runtime_package() {
        return Err(RuntimeCapabilityError::UnauthorizedPackage);
    }
    manifest
        .validate()
        .map_err(|_| RuntimeCapabilityError::InvalidManifest)?;
    if manifest.trusted_runtime_package.as_ref() != Some(package) {
        return Err(RuntimeCapabilityError::InvalidManifest);
    }

    let expected_sources = canonical_runtime_sources();
    let expected_hash = canonical_runtime_source_hash();
    let actual_hash =
        canonical_source_hash(sources).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
    if sources.len() != expected_sources.len() || actual_hash != expected_hash {
        return Err(RuntimeCapabilityError::SourceSetMismatch);
    }

    Ok(RuntimeIntrinsicCapability {
        source_hash: expected_hash,
        source_paths: expected_sources
            .into_iter()
            .map(|unit| unit.logical_path)
            .collect(),
        intrinsics: manifest.trusted_runtime_intrinsics.clone(),
    })
}
