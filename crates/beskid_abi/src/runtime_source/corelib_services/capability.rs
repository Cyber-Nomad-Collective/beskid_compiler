use crate::abi_v5::{AbiManifestV5, canonical_runtime_package, canonical_source_hash};

use super::super::capabilities::RuntimeCapabilityError;
use super::super::sources::canonical_corelib_service_sources;
use super::service_table::{CORELIB_SERVICES, CorelibService};

/// Compiler-owned proof that a unit belongs to the embedded Corelib service corpus.
///
/// This has a separate type and constructor from [`super::RuntimeIntrinsicCapability`], so Corelib
/// never inherits raw bootstrap intrinsic authority.
#[derive(Debug)]
pub struct CorelibServiceProof {
    source_hash: String,
    source_paths: Vec<String>,
}

impl CorelibServiceProof {
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }

    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.source_paths.iter().any(|candidate| candidate == logical_path)
    }
}

#[derive(Debug)]
pub struct CorelibServiceCapability {
    proof: CorelibServiceProof,
}

impl CorelibServiceCapability {
    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.proof.authorizes_source(logical_path)
    }

    pub fn service_for_source(&self, logical_path: &str, name: &str) -> Option<CorelibService> {
        self.authorizes_source(logical_path)
            .then(|| {
                CORELIB_SERVICES
                    .iter()
                    .copied()
                    .find(|service| service.name == name && service.source_path == logical_path)
            })
            .flatten()
    }

    pub fn services(&self) -> &'static [CorelibService] {
        CORELIB_SERVICES
    }
}

/// Mint the distinct Corelib syscall service capability from the compiler-embedded source.
///
/// Callers still have to prove their assembled unit exactly matches this corpus before the
/// capability can be attached to syntax facts. The ABI manifest is validated here to prevent a
/// drifted target contract from being combined with compiler-owned services.
pub fn canonical_corelib_service_capability(
    manifest: &AbiManifestV5,
) -> Result<CorelibServiceCapability, RuntimeCapabilityError> {
    manifest.validate().map_err(|_| RuntimeCapabilityError::InvalidManifest)?;
    if manifest.trusted_runtime_package.as_ref() != Some(&canonical_runtime_package()) {
        return Err(RuntimeCapabilityError::InvalidManifest);
    }
    let sources = canonical_corelib_service_sources();
    let source_hash = canonical_source_hash(&sources).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
    Ok(CorelibServiceCapability {
        proof: CorelibServiceProof {
            source_hash,
            source_paths: sources.into_iter().map(|unit| unit.logical_path).collect(),
        },
    })
}

/// Backwards-compatible spelling for callers that only need the syscall subset.
///
/// The returned capability remains source-scoped; it cannot authorize assertion services for a
/// syscall unit.
pub fn canonical_corelib_syscall_service_capability(
    manifest: &AbiManifestV5,
) -> Result<CorelibServiceCapability, RuntimeCapabilityError> {
    canonical_corelib_service_capability(manifest)
}
