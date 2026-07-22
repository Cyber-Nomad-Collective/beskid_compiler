//! Compiler-embedded authority for canonical Beskid runtime sources.
//!
//! This module grants no ambient or serializable package capability. The frontend may use the
//! returned token only for AST nodes from the exact embedded source corpus.

use std::path::Path;

use crate::abi_v5::{
    AbiManifestV5, RuntimeIntrinsic, SourceUnit, canonical_runtime_package, canonical_source_hash,
};
use crate::runtime_kit::{
    BuildProfile, ResolvedRuntimeKit, RuntimeKitBuildError, RuntimeKitBuildRequest,
    RuntimeKitResolutionError, build_runtime_kit, resolve_installed_runtime_kit,
};

pub const CANONICAL_BOOTSTRAP_SOURCE_PATH: &str = "src/Runtime/Bootstrap.bd";
/// Canonical Foundation syscall facade eligible for Corelib service authority.
pub const CANONICAL_CORELIB_SYSCALL_SOURCE_PATH: &str = "Core/Syscall/Syscall.bd";
/// Canonical Foundation assertion helper eligible to import the panic runtime service.
pub const CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH: &str = "Testing/Assert.bd";
/// Canonical Foundation output helper eligible to import the panic runtime service.
pub const CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH: &str = "Core/Output/Output.bd";

const CANONICAL_BOOTSTRAP_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/beskid/src/Runtime/Bootstrap.bd"
));

const CANONICAL_CORELIB_SYSCALL_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corelib/packages/foundation/src/Core/Syscall/Syscall.bd"
));

const CANONICAL_FOUNDATION_ASSERT_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corelib/packages/foundation/src/Testing/Assert.bd"
));

const CANONICAL_FOUNDATION_OUTPUT_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corelib/packages/foundation/src/Core/Output/Output.bd"
));

/// The runtime source corpus built into this compiler version.
pub fn canonical_runtime_sources() -> Vec<SourceUnit> {
    vec![SourceUnit {
        logical_path: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
        source: CANONICAL_BOOTSTRAP_SOURCE.into(),
    }]
}

/// The compiler-embedded Corelib syscall facade. This is deliberately a distinct source corpus
/// from the runtime bootstrap: Corelib services must never borrow runtime-intrinsic authority.
pub fn canonical_corelib_syscall_sources() -> Vec<SourceUnit> {
    vec![SourceUnit {
        logical_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
        source: CANONICAL_CORELIB_SYSCALL_SOURCE.into(),
    }]
}

/// Compiler-embedded Foundation units eligible for distinct ABI service authority.
pub fn canonical_corelib_service_sources() -> Vec<SourceUnit> {
    let mut sources = canonical_corelib_syscall_sources();
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_ASSERT_SOURCE.into(),
    });
    sources.push(SourceUnit {
        logical_path: CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH.into(),
        source: CANONICAL_FOUNDATION_OUTPUT_SOURCE.into(),
    });
    sources
}

/// The canonical compiler-owned source file for one Foundation service unit.
///
/// Authority is tied to this checked-in file identity as well as embedded bytes and logical
/// module path. A user project that copies `Testing/Assert.bd` cannot acquire it.
///
/// The returned path is lexically normalized so `Path::starts_with` / equality against
/// resolved Foundation `source_root` values succeed. Leaving `../..` from
/// `CARGO_MANIFEST_DIR` intact made materialized Corelib deps drop panic/syscall provenance
/// and fall through to Dynamic `__panic_str` (Corelib gate).
pub fn canonical_corelib_service_source_path(logical_path: &str) -> Option<std::path::PathBuf> {
    let relative = match logical_path {
        CANONICAL_CORELIB_SYSCALL_SOURCE_PATH => "Core/Syscall/Syscall.bd",
        CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH => "Testing/Assert.bd",
        CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH => "Core/Output/Output.bd",
        _ => return None,
    };
    Some(normalize_lexically(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corelib/packages/foundation/src")
            .join(relative),
    ))
}

/// Collapse `.` / `..` components without requiring the path to exist on disk.
fn normalize_lexically(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// One ABI-facing service used by a compiler-owned Corelib source unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorelibService {
    pub name: &'static str,
    pub symbol: &'static str,
    pub source_path: &'static str,
}

const CORELIB_SERVICES: &[CorelibService] = &[
    CorelibService {
        name: "__syscall_write",
        symbol: "syscall_write",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_read",
        symbol: "syscall_read",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_write_bytes",
        symbol: "syscall_write_bytes",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_read_bytes",
        symbol: "syscall_read_bytes",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "panic_str",
        source_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "panic_str",
        source_path: CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH,
    },
];

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
        self.source_paths
            .iter()
            .any(|candidate| candidate == logical_path)
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

/// Compiler-owned proof that a unit belongs to the embedded Corelib service corpus.
///
/// This has a separate type and constructor from [`RuntimeIntrinsicCapability`], so Corelib
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
        self.source_paths
            .iter()
            .any(|candidate| candidate == logical_path)
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

impl RuntimeIntrinsicCapability {
    pub fn source_hash(&self) -> &str {
        self.proof.source_hash()
    }

    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.proof.authorizes_source(logical_path)
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

/// Mint an opaque proof only for the exact runtime corpus embedded in this compiler.
///
/// Syntax program assemblies never carry this proof. A caller may present a corpus for
/// verification, but matching a package name or a source path alone cannot mint it.
pub fn prove_canonical_runtime_corpus(
    sources: &[SourceUnit],
    manifest: &AbiManifestV5,
) -> Result<CanonicalRuntimeProof, RuntimeCapabilityError> {
    manifest
        .validate()
        .map_err(|_| RuntimeCapabilityError::InvalidManifest)?;
    if manifest.trusted_runtime_package.as_ref() != Some(&canonical_runtime_package()) {
        return Err(RuntimeCapabilityError::InvalidManifest);
    }

    let expected_sources = canonical_runtime_sources();
    let expected_hash = canonical_runtime_source_hash();
    let actual_hash =
        canonical_source_hash(sources).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
    if sources.len() != expected_sources.len() || actual_hash != expected_hash {
        return Err(RuntimeCapabilityError::SourceSetMismatch);
    }

    Ok(CanonicalRuntimeProof {
        source_hash: expected_hash,
        source_paths: expected_sources
            .into_iter()
            .map(|unit| unit.logical_path)
            .collect(),
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
    Ok(RuntimeIntrinsicCapability {
        proof,
        intrinsics: manifest.trusted_runtime_intrinsics.clone(),
    })
}

/// Mint the distinct Corelib syscall service capability from the compiler-embedded source.
///
/// Callers still have to prove their assembled unit exactly matches this corpus before the
/// capability can be attached to syntax facts. The ABI manifest is validated here to prevent a
/// drifted target contract from being combined with compiler-owned services.
pub fn canonical_corelib_service_capability(
    manifest: &AbiManifestV5,
) -> Result<CorelibServiceCapability, RuntimeCapabilityError> {
    manifest
        .validate()
        .map_err(|_| RuntimeCapabilityError::InvalidManifest)?;
    if manifest.trusted_runtime_package.as_ref() != Some(&canonical_runtime_package()) {
        return Err(RuntimeCapabilityError::InvalidManifest);
    }
    let sources = canonical_corelib_service_sources();
    let source_hash =
        canonical_source_hash(&sources).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
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
