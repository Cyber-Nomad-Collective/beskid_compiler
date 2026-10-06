use beskid_pipeline::{
    observe_phase_result,
    phases::{AOT_LINK, AOT_RUNTIME},
};
use cargo_cross::env::sanitize_cargo_env;

use crate::error::{AotError, AotResult};
use crate::linker::{LinkRequest, link};
use crate::object_module::EXECUTABLE_PROGRAM_ENTRY;
use crate::runtime::merged_link_libraries;
use crate::runtime::{RuntimeBuildRequest, RuntimeLinkage, prepare_runtime};

use super::model::{AotBuildRequest, AotBuildResult, BuildOutputKind};
use super::object_stage::{ObjectStageResult, emit_object_stage};
use super::validation::{native_link_entrypoint, requires_entrypoint, validate_request};
/// Emit a single object file; fails unless `req.output_kind` is [`BuildOutputKind::ObjectOnly`].
pub fn emit_object_only(req: AotBuildRequest) -> AotResult<AotBuildResult> {
    if req.output_kind != BuildOutputKind::ObjectOnly {
        return Err(AotError::InvalidRequest {
            message: "emit_object_only requires BuildOutputKind::ObjectOnly".to_owned(),
        });
    }
    build(req)
}

/// Run object emission, optional runtime preparation, and linking per `req.output_kind`.
pub fn build(req: AotBuildRequest) -> AotResult<AotBuildResult> {
    // Sanitize the cargo environment before building to avoid leaking
    // host toolchain variables into cross-compilation invocations.
    sanitize_cargo_env();
    validate_request(&req)?;

    let mut object_stage = emit_object_stage(&req)?;

    if req.output_kind == BuildOutputKind::ObjectOnly {
        return Ok(AotBuildResult {
            object_path: object_stage.object_path,
            final_path: None,
            exported_symbols: object_stage.exported_symbols,
            linker_invocation: None,
        });
    }

    if requires_entrypoint(req.output_kind) {
        ensure_entrypoint_emitted(&req, &object_stage)?;
    }
    let runtime_request = RuntimeBuildRequest {
        kit: req.runtime.clone().expect("validated linked output runtime kit"),
        linkage: if req.artifact.dynamic_initialization.is_some() {
            RuntimeLinkage::GlueSharedProviderV1
        } else {
            RuntimeLinkage::CanonicalStatic
        },
    };
    let control = super::NativeExecutionControl::new(
        std::time::Instant::now() + std::time::Duration::from_secs(300),
        std::sync::Arc::new(|| false),
    );
    let initialization = super::glue::compile_dynamic_initialization(
        &req.artifact,
        &runtime_request,
        req.output_path.parent().unwrap_or_else(|| std::path::Path::new(".")),
        &control,
    )?;
    if let Some(initialization) = &initialization {
        initialization.verify_object()?;
        object_stage.additional_object_paths.push(initialization.object_path().to_owned());
    }
    let runtime = prepare_runtime_stage(&req)?;
    let link_result = link_stage(&req, &object_stage, &runtime)?;

    Ok(AotBuildResult {
        object_path: object_stage.object_path,
        final_path: Some(link_result.output_path),
        exported_symbols: link_result.exported_symbols,
        linker_invocation: Some(link_result.command_line),
    })
}

fn ensure_entrypoint_emitted(req: &AotBuildRequest, object_stage: &ObjectStageResult) -> AotResult<()> {
    if object_stage.executable_program_entry.as_deref() == Some(EXECUTABLE_PROGRAM_ENTRY) {
        return Ok(());
    }
    Err(AotError::MissingEntrypoint { symbol: req.entrypoint.clone() })
}

fn prepare_runtime_stage(req: &AotBuildRequest) -> AotResult<crate::runtime::RuntimeArtifact> {
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_RUNTIME, || {
        prepare_runtime(&RuntimeBuildRequest {
            kit: req.runtime.clone().expect("validated linked output runtime kit"),
            linkage: if req.artifact.dynamic_initialization.is_some() {
                RuntimeLinkage::GlueSharedProviderV1
            } else {
                RuntimeLinkage::CanonicalStatic
            },
        })
    })
}

fn link_stage(
    req: &AotBuildRequest,
    object_stage: &ObjectStageResult,
    runtime: &crate::runtime::RuntimeArtifact,
) -> AotResult<crate::linker::LinkResult> {
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_LINK, || {
        link(&LinkRequest {
            target_triple: req.target_triple.clone(),
            output_kind: req.output_kind,
            output_path: req.output_path.clone(),
            object_path: object_stage.object_path.clone(),
            additional_object_paths: object_stage.additional_object_paths.clone(),
            runtime: Some(crate::linker::RuntimeLinkInput::try_from(runtime)?),
            host_staticlib: None,
            entrypoint_symbol: object_stage
                .executable_program_entry
                .clone()
                .unwrap_or_else(|| native_link_entrypoint(&req.entrypoint).to_owned()),
            exported_symbols: object_stage.exported_symbols.clone(),
            link_mode: req.link_mode,
            verbose: req.verbose_link,
            external_libraries: merged_link_libraries(&req.external_libraries, &runtime.platform_libraries),
            library_search_paths: req.library_search_paths.clone(),
        })
    })
}
