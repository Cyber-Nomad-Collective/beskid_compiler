use anyhow::Result;
use beskid_analysis::resolve::ItemKind;
use beskid_analysis::services::ResolvedInput;
use beskid_pipeline::PipelineObserver;

use crate::Engine;
use crate::jit_callable::{EntryReturnKind, JitCallable};

/// Parse, lower, JIT-compile, and run `entrypoint` (no-arg function or test); returns a string summary of the return value.
pub fn run_entrypoint(
    source_path: &std::path::Path,
    source: &str,
    entrypoint: &str,
) -> Result<String> {
    run_entrypoint_with_pipeline(source_path, source, entrypoint, None)
}

/// Same as [`run_entrypoint`] with optional pipeline observation for codegen and JIT phases.
pub fn run_entrypoint_with_pipeline(
    source_path: &std::path::Path,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let source_path =
        beskid_codegen::materialize_source_path_for_lowering(source_path, source)?;
    let compile_plan = beskid_analysis::services::compile_plan_for_input_path(&source_path)
        .or_else(|| {
            Some(beskid_analysis::services::synthetic_compile_plan_for_source(
                &source_path,
            ))
        });
    run_resolved_entrypoint_with_pipeline(
        &ResolvedInput {
            source_path,
            source: source.to_string(),
            compile_plan,
            prepared_workspace: None,
            workspace_summary: None,
            assembly: None,
        },
        entrypoint,
        pipeline,
    )
}

/// JIT-compile and run using a pre-built front-end (avoids re-running semantic analysis).
pub fn run_entrypoint_from_front_end_with_pipeline(
    front: &beskid_analysis::services::FrontEndTypedResult,
    source_name: &str,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let mut engine = Engine::try_new()?;
    run_entrypoint_from_front_end_with_engine(
        &mut engine,
        front,
        source_name,
        source,
        entrypoint,
        pipeline,
    )
}

/// Like [`run_entrypoint_from_front_end_with_pipeline`] but reuses an existing [`Engine`].
pub fn run_entrypoint_from_front_end_with_engine(
    engine: &mut Engine,
    front: &beskid_analysis::services::FrontEndTypedResult,
    source_name: &str,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let artifact = beskid_codegen::entrypoint_artifact_from_front_end(
        front.as_lower_input(),
        source_name,
        source,
        entrypoint,
        pipeline,
    )?;

    run_jitted_entrypoint(
        engine,
        &front.resolution,
        &front.typed,
        &artifact,
        entrypoint,
        pipeline,
    )
}

/// JIT-compile and run using a fully resolved project input (same assembly path as `beskid build`).
pub fn run_resolved_entrypoint_with_pipeline(
    resolved: &ResolvedInput,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    run_resolved_entrypoint_with_pipeline_inner(resolved, entrypoint, true, pipeline)
}

/// Like [`run_resolved_entrypoint_with_pipeline`] but skips semantic diagnostics (after gate).
pub fn run_resolved_entrypoint_after_gate_with_pipeline(
    resolved: &ResolvedInput,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    run_resolved_entrypoint_with_pipeline_inner(resolved, entrypoint, false, pipeline)
}

fn run_resolved_entrypoint_with_pipeline_inner(
    resolved: &ResolvedInput,
    entrypoint: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let lowered = beskid_codegen::lower_resolved_entrypoint_with_pipeline(
        resolved,
        Some(entrypoint),
        with_diagnostics,
        pipeline,
    )?;

    let mut engine = Engine::try_new()?;
    run_jitted_entrypoint(
        &mut engine,
        &lowered.resolution,
        &lowered.typed,
        &lowered.artifact,
        entrypoint,
        pipeline,
    )
}

fn run_jitted_entrypoint(
    engine: &mut Engine,
    resolution: &beskid_analysis::resolve::Resolution,
    typed: &beskid_analysis::types::TypeResult,
    artifact: &beskid_codegen::CodegenArtifact,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    engine
        .compile_artifact_with_pipeline(artifact, pipeline)
        .map_err(|err| anyhow::anyhow!("JIT compile failed: {err}"))?;

    let entrypoint_info = resolution
        .items
        .iter()
        .find(|item| {
            entrypoint_matches_item(item, entrypoint)
                && (item.kind == ItemKind::Function || item.kind == ItemKind::Test)
        })
        .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;

    let jit_symbol = beskid_codegen::jit_symbol_for_item(resolution, entrypoint_info.id);

    let signature = typed
        .function_signatures
        .get(&entrypoint_info.id)
        .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;

    if !signature.params.is_empty() {
        return Err(anyhow::anyhow!(
            "Entrypoint `{entrypoint}` must take no parameters"
        ));
    }

    let return_info = typed
        .types
        .get(signature.return_type)
        .ok_or_else(|| anyhow::anyhow!("Missing return type for `{entrypoint}`"))?;

    let ptr = unsafe { engine.entrypoint_ptr(&jit_symbol) }
        .map_err(|err| anyhow::anyhow!("Entrypoint lookup failed: {err}"))?;
    if ptr.is_null() {
        return Err(anyhow::anyhow!(
            "Entrypoint `{entrypoint}` returned null pointer"
        ));
    }

    let return_kind = EntryReturnKind::from_type_info(return_info);

    let output = JitCallable::execute_as_i64(ptr, return_kind);

    Ok(JitCallable::format_i64_result(output, return_kind))
}

fn entrypoint_matches_item(item: &beskid_analysis::resolve::ItemInfo, entrypoint: &str) -> bool {
    if item.name == entrypoint {
        return true;
    }
    if !entrypoint.contains("::") {
        return false;
    }
    let Some(short) = entrypoint.rsplit("::").next() else {
        return false;
    };
    item.name == short
}
