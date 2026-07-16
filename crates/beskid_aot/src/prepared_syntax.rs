//! AOT source-input handoff through the shared prepared-syntax codegen authority.

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::FrontEndTypedResult;

use crate::error::AotResult;

/// Lower a prepared frontend snapshot using the exact ISA selected for AOT object emission.
/// Runtime-kit validation remains in [`crate::build`], after this artifact boundary.
pub fn lower_prepared_syntax_entrypoint(
    front: &FrontEndTypedResult,
    entrypoint: &str,
    target: TargetMetadata,
) -> AotResult<beskid_codegen::CodegenArtifact> {
    let isa = crate::object_module::object_target_isa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_syntax_entrypoint(db, front, entrypoint, target, isa.as_ref())
            .map(|lowered| lowered.artifact)
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })
}

/// Lower the compiler-owned canonical runtime source through the same prepared-syntax AOT
/// boundary used by hosts. Caller-provided sources never receive the runtime intrinsic authority.
pub fn lower_canonical_runtime_prepared_syntax(
    target: TargetMetadata,
) -> AotResult<beskid_codegen::CodegenArtifact> {
    let isa = crate::object_module::object_target_isa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_canonical_runtime_prepared_syntax(db, target, isa.as_ref())
            .map_err(|error| crate::error::AotError::InvalidRequest {
                message: error.to_string(),
            })
    })
}
