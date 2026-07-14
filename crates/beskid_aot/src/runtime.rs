//! Beskid runtime static library resolved from one exact ABI-v5 runtime kit.

use std::path::PathBuf;

use crate::api::RuntimeKitRequest;
use crate::error::AotResult;

/// Runtime static library and verified ABI export allowlist for the linker.
#[derive(Debug, Clone)]
pub struct RuntimeArtifact {
    pub staticlib_path: PathBuf,
    pub exported_symbols: Vec<String>,
}

/// Exact runtime-kit identity requested by the AOT linker.
#[derive(Debug, Clone)]
pub struct RuntimeBuildRequest {
    pub kit: RuntimeKitRequest,
}

/// Resolve the static archive from the request's hash-verified ABI-v5 kit.
pub fn prepare_runtime(req: &RuntimeBuildRequest) -> AotResult<RuntimeArtifact> {
    let kit =
        crate::bundled::resolve_aot_runtime_kit(&req.kit.prefix, &req.kit.target, req.kit.profile)?;
    Ok(RuntimeArtifact {
        staticlib_path: kit.static_library,
        exported_symbols: kit.metadata.export_allowlist,
    })
}
