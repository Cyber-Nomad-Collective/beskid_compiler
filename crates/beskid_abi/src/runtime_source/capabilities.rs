use crate::abi_v5::{AbiManifestV5, RuntimeIntrinsic, SourceUnit, canonical_runtime_package, canonical_source_hash};

use super::sources::{canonical_runtime_source_hash, canonical_runtime_sources};

/// A compiler-owned proof that a node belongs to the exact canonical runtime corpus.
///
/// Deliberately has no public constructor and does not implement serialization.
#[derive(Debug)]
pub struct CanonicalRuntimeProof {
    source_hash: String,
    source_paths: Vec<String>,
}

impl CanonicalRuntimeProof {
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }

    /// Check a logical path against the exact corpus validated when this proof was minted.
    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.source_paths.iter().any(|candidate| candidate == logical_path)
    }
}

/// Trusted intrinsic selection derived from a [`CanonicalRuntimeProof`].
///
/// It intentionally cannot be constructed from an analysis assembly or package identity.
#[derive(Debug)]
pub struct RuntimeIntrinsicCapability {
    proof: CanonicalRuntimeProof,
    intrinsics: Vec<RuntimeIntrinsic>,
}

impl RuntimeIntrinsicCapability {
    pub fn source_hash(&self) -> &str {
        self.proof.source_hash()
    }

    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.proof.authorizes_source(logical_path)
    }

    pub fn intrinsic_for_source(&self, logical_path: &str, name: &str) -> Option<&RuntimeIntrinsic> {
        if !self.authorizes_source(logical_path) {
            return None;
        }
        self.intrinsics.iter().find(|intrinsic| intrinsic.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapabilityError {
    SourceSetMismatch,
    InvalidManifest,
}

/// Mint an opaque proof only for the exact runtime corpus embedded in this compiler.
///
/// Syntax program assemblies never carry this proof. A caller may present a corpus for
/// verification, but matching a package name or a source path alone cannot mint it.
pub fn prove_canonical_runtime_corpus(
    sources: &[SourceUnit],
    manifest: &AbiManifestV5,
) -> Result<CanonicalRuntimeProof, RuntimeCapabilityError> {
    manifest.validate().map_err(|_| RuntimeCapabilityError::InvalidManifest)?;
    if manifest.trusted_runtime_package.as_ref() != Some(&canonical_runtime_package()) {
        return Err(RuntimeCapabilityError::InvalidManifest);
    }

    let expected_sources = canonical_runtime_sources();
    let expected_hash = canonical_runtime_source_hash();
    let actual_hash = canonical_source_hash(sources).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
    if sources.len() != expected_sources.len() || actual_hash != expected_hash {
        return Err(RuntimeCapabilityError::SourceSetMismatch);
    }

    Ok(CanonicalRuntimeProof {
        source_hash: expected_hash,
        source_paths: expected_sources.into_iter().map(|unit| unit.logical_path).collect(),
    })
}

/// Produce the only intrinsic authority factory exposed by the ABI layer.
///
/// This factory uses the compiler-embedded corpus rather than accepting an analysis assembly,
/// so ordinary projects cannot gain runtime identity by imitating its path, name, or source.
pub fn canonical_runtime_intrinsic_capability(
    manifest: &AbiManifestV5,
) -> Result<RuntimeIntrinsicCapability, RuntimeCapabilityError> {
    let proof = prove_canonical_runtime_corpus(&canonical_runtime_sources(), manifest)?;
    Ok(RuntimeIntrinsicCapability { proof, intrinsics: manifest.trusted_runtime_intrinsics.clone() })
}
