//! Exact validated runtime linkage; Glue requires one canonical shared provider.

use std::path::PathBuf;

use beskid_abi::abi_v5::AbiManifestV5;

use crate::api::RuntimeKitRequest;
use crate::error::AotResult;

/// Runtime static library and verified ABI export allowlist for the linker.
#[derive(Debug, Clone)]
pub struct RuntimeArtifact {
    pub link_path: PathBuf,
    pub linkage: RuntimeLinkage,
    pub shared_library_path: Option<PathBuf>,
    pub owner_issuer_source_sha256: Option<String>,
    pub exported_symbols: Vec<String>,
    /// Platform libraries are derived from the exact ABI-v5 contract in the
    /// installed kit request, never from a handwritten linker exception.
    pub platform_libraries: Vec<String>,
}

/// Exact runtime-kit identity requested by the AOT linker.
#[derive(Debug, Clone)]
pub struct RuntimeBuildRequest {
    pub kit: RuntimeKitRequest,
    pub linkage: RuntimeLinkage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLinkage {
    CanonicalStatic,
    GlueSharedProviderV1,
}

/// Resolve the static archive from the request's hash-verified ABI-v5 kit.
pub fn prepare_runtime(req: &RuntimeBuildRequest) -> AotResult<RuntimeArtifact> {
    match req.linkage {
        RuntimeLinkage::CanonicalStatic => {
            let kit = crate::bundled::resolve_aot_runtime_kit(&req.kit.prefix, &req.kit.target, req.kit.profile)?;
            Ok(RuntimeArtifact {
                link_path: kit.static_library,
                linkage: req.linkage,
                shared_library_path: None,
                owner_issuer_source_sha256: None,
                exported_symbols: kit.metadata.export_allowlist,
                platform_libraries: canonical_runtime_platform_libraries(&req.kit.target),
            })
        }
        RuntimeLinkage::GlueSharedProviderV1 => {
            let provider = beskid_abi::runtime_kit::resolve_glue_shared_provider(
                &req.kit.prefix,
                &req.kit.target,
                req.kit.profile,
            )
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })?;
            Ok(RuntimeArtifact {
                link_path: provider.link_library,
                linkage: req.linkage,
                shared_library_path: Some(provider.shared_library),
                owner_issuer_source_sha256: Some(provider.manifest.issuer_source_sha256),
                exported_symbols: provider.kit.metadata.export_allowlist,
                platform_libraries: canonical_runtime_platform_libraries(&req.kit.target),
            })
        }
    }
}

/// The canonical ABI-v5 manifest is the one authority for the native libraries
/// required by a runtime target. Keep this separate from user-declared extern
/// libraries: those remain validated against source-level extern imports.
pub(crate) fn canonical_runtime_platform_libraries(target: &beskid_abi::abi_v5::TargetMetadata) -> Vec<String> {
    let mut libraries = AbiManifestV5::canonical_runtime(target.clone())
        .platform_imports
        .into_iter()
        .map(|entry| entry.library)
        .collect::<Vec<_>>();
    libraries.sort();
    libraries.dedup();
    libraries
}

pub(crate) fn merged_link_libraries(primary: &[String], runtime: &[String]) -> Vec<String> {
    let mut libraries = primary.to_vec();
    libraries.extend(runtime.iter().cloned());
    libraries.sort();
    libraries.dedup();
    libraries
}

#[cfg(test)]
mod tests {
    use beskid_abi::abi_v5::TargetMetadata;

    use super::canonical_runtime_platform_libraries;

    #[test]
    fn canonical_windows_runtime_libraries_derive_from_manifest_imports() {
        let target = TargetMetadata::for_triple("x86_64-pc-windows-msvc").expect("supported target");
        let libraries = canonical_runtime_platform_libraries(&target);
        assert!(libraries.contains(&"kernel32".to_owned()));
        assert!(libraries.contains(&"ws2_32".to_owned()));
    }

    #[test]
    fn canonical_unix_runtime_libraries_do_not_inherit_windows_winsock() {
        let target = TargetMetadata::for_triple("x86_64-unknown-linux-gnu").expect("supported target");
        assert!(!canonical_runtime_platform_libraries(&target).contains(&"ws2_32".to_owned()));
    }
}
