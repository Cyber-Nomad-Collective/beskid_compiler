//! AOT source-input handoff through the shared prepared-syntax codegen authority.

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::FrontEndTypedResult;
use std::sync::{Mutex, PoisonError};

use crate::error::AotResult;

/// Lower a prepared frontend snapshot using the exact ISA selected for AOT object emission.
/// Runtime-kit validation remains in [`crate::build`], after this artifact boundary.
pub fn lower_prepared_syntax_entrypoint(
    front: &FrontEndTypedResult,
    entrypoint: &str,
    target: TargetMetadata,
) -> AotResult<beskid_codegen::CodegenArtifact> {
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_syntax_entrypoint(db, front, entrypoint, target, isa.as_ref())
            .map(|lowered| lowered.artifact)
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })
}

/// Lower every executable item in a prepared frontend snapshot using the exact ISA selected for
/// AOT object emission.
pub fn lower_prepared_syntax_module(
    front: &FrontEndTypedResult,
    target: TargetMetadata,
) -> AotResult<beskid_codegen::CodegenArtifact> {
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_syntax_module(db, front, target, isa.as_ref())
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })
}

/// Lower the compiler-owned canonical runtime source through the same prepared-syntax AOT
/// boundary used by hosts. Caller-provided sources never receive the runtime intrinsic authority.
///
/// The canonical corpus is embedded in the compiler, so its lowering for one target is fixed for
/// the life of the process. A host that publishes several kits in one process (the debug and
/// release kits of a bundle, or the kits of a test binary) reuses the first lowering instead of
/// repeating it; each lowering of the corpus takes tens of seconds. Failures are not retained.
pub fn lower_canonical_runtime_prepared_syntax(target: TargetMetadata) -> AotResult<beskid_codegen::CodegenArtifact> {
    static LOWERED: Mutex<Vec<(TargetMetadata, beskid_codegen::CodegenArtifact)>> = Mutex::new(Vec::new());
    // Holding the lock while lowering makes concurrent callers for one target wait for the
    // first result rather than lower the corpus twice.
    let mut lowered = LOWERED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, artifact)) = lowered.iter().find(|(cached, _)| *cached == target) {
        return Ok(artifact.clone());
    }
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    let artifact = beskid_queries::with_db(|db| {
        beskid_codegen::lower_canonical_runtime_prepared_syntax(db, target.clone(), isa.as_ref())
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })?;
    lowered.push((target, artifact.clone()));
    Ok(artifact)
}
