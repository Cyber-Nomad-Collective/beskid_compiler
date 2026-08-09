use crate::abi_v5::{AbiManifestV5, canonical_runtime_package, canonical_source_hash};

use super::capabilities::RuntimeCapabilityError;
use super::sources::{
    CANONICAL_CORELIB_ARGS_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    CANONICAL_FOUNDATION_ERROR_SOURCE_PATH, CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH, canonical_corelib_service_sources,
};

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
        CANONICAL_CORELIB_ARGS_SOURCE_PATH => "Core/Args/Args.bd",
        CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH => "Testing/Assert.bd",
        CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH => "Core/Output/Output.bd",
        CANONICAL_FOUNDATION_ERROR_SOURCE_PATH => "Core/Error/Error.bd",
        _ => return None,
    };
    Some(normalize_lexically(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src").join(relative),
    ))
}

/// Collapse `.` / `..` components without requiring the path to exist on disk.
fn normalize_lexically(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::new();
    path.components().for_each(|component| match component {
        Component::ParentDir => {
            out.pop();
        }
        Component::CurDir => {}
        other => out.push(other.as_os_str()),
    });
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
    CorelibService { name: "__args_count", symbol: "args_count", source_path: CANONICAL_CORELIB_ARGS_SOURCE_PATH },
    CorelibService { name: "__args_get", symbol: "args_get", source_path: CANONICAL_CORELIB_ARGS_SOURCE_PATH },
    CorelibService { name: "__panic_str", symbol: "panic_str", source_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH },
    CorelibService { name: "__panic_str", symbol: "panic_str", source_path: CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH },
    CorelibService { name: "__panic_str", symbol: "panic_str", source_path: CANONICAL_FOUNDATION_ERROR_SOURCE_PATH },
];

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
