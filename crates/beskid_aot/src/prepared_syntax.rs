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
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_syntax_entrypoint(db, front, entrypoint, target, isa.as_ref())
            .map(|lowered| lowered.into_artifact())
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

/// Lower the library output of a prepared frontend snapshot (shared, static, and object builds)
/// using the exact ISA selected for AOT object emission. The library owns its own root units; see
/// [`beskid_codegen::lower_prepared_syntax_library`] for the emission and export set.
pub fn lower_prepared_syntax_library(
    front: &FrontEndTypedResult,
    target: TargetMetadata,
) -> AotResult<beskid_codegen::CodegenArtifact> {
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_syntax_library(db, front, target, isa.as_ref())
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })
}

/// Lower the compiler-owned canonical runtime source through the same prepared-syntax AOT
/// boundary used by hosts. Caller-provided sources never receive the runtime intrinsic authority.
pub fn lower_canonical_runtime_prepared_syntax(target: TargetMetadata) -> AotResult<beskid_codegen::CodegenArtifact> {
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_canonical_runtime_prepared_syntax(db, target, isa.as_ref())
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })
}

/// Lower a selected native test closure once using the AOT target ISA.
pub fn lower_prepared_syntax_entrypoints(
    front: &FrontEndTypedResult,
    entrypoints: &[String],
    target: TargetMetadata,
) -> AotResult<beskid_codegen::PreparedSyntaxEntrypoints> {
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_syntax_entrypoints(db, front, entrypoints, target, isa.as_ref())
            .map_err(|error| crate::error::AotError::InvalidRequest { message: error.to_string() })
    })
}

/// Produce frozen C ABI Mod adapters from their canonical prepared source SDK authority.
pub fn lower_prepared_native_mod(
    front: &FrontEndTypedResult,
    target: TargetMetadata,
) -> AotResult<beskid_codegen::PreparedNativeMod> {
    let isa = crate::object_module::ObjectTargetIsa(target.triple.as_str())?;
    beskid_queries::with_db(|db| {
        beskid_codegen::lower_prepared_native_mod(db, front, target, isa.as_ref())
            // Alternate formatting keeps the whole anyhow context chain (phase, site, original
            // semantic error text) instead of only the outermost context.
            .map_err(|error| crate::error::AotError::InvalidRequest { message: format!("{error:#}") })
    })
}

/// Glue producer entry: run `produce` against the registered prepared-front codegen input.
///
/// The prepared front end is materialized once through the shared prepared-syntax authority
/// ([`beskid_codegen::with_prepared_glue_input`]) inside the session database. `produce` receives
/// exactly that registered [`beskid_codegen::CodegenInput`] and its executable
/// [`beskid_codegen::module_emission::SyntaxModuleItem`] closure, which it passes to
/// [`crate::api::glue::build_rust_owner`] and [`crate::api::glue::build_glue_artifact`]. Their
/// witnesses must be produced inside `produce`, because the input borrows the session database.
///
/// The target must be an AOT object target; unsupported targets and every materializer failure
/// fail closed before `produce` runs.
pub fn with_prepared_glue_input<T, E: From<crate::error::AotError>>(
    front: &FrontEndTypedResult,
    target: TargetMetadata,
    produce: impl FnOnce(
        &beskid_codegen::CodegenInput<'_>,
        &[beskid_codegen::module_emission::SyntaxModuleItem],
    ) -> Result<T, E>,
) -> Result<T, E> {
    crate::object_module::ObjectTargetIsa(target.triple.as_str()).map_err(E::from)?;
    beskid_queries::with_db(|db| {
        let mut produced = None;
        beskid_codegen::with_prepared_glue_input(db, front, target, |input, items| {
            produced = Some(produce(input, items));
            Ok(())
        })
        .map_err(|error| E::from(crate::error::AotError::InvalidRequest { message: error.to_string() }))?;
        produced.unwrap_or_else(|| {
            Err(E::from(crate::error::AotError::InvalidRequest {
                message: "registered Glue input completed without running its producer".to_owned(),
            }))
        })
    })
}
