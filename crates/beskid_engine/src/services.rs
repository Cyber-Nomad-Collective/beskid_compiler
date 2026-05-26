use anyhow::Result;
use beskid_analysis::resolve::ItemKind;
use beskid_analysis::services::ResolvedInput;
use beskid_pipeline::PipelineObserver;

use crate::Engine;
use crate::jit_callable::JitCallable;

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
    run_resolved_entrypoint_with_pipeline(
        &ResolvedInput {
            source_path: source_path.to_path_buf(),
            source: source.to_string(),
            compile_plan: beskid_analysis::services::compile_plan_for_input_path(source_path),
            prepared_workspace: None,
            workspace_summary: None,
            assembly: None,
        },
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
    let lowered =
        beskid_codegen::lower_resolved_input_with_pipeline(resolved, false, pipeline)?;

    let mut engine = Engine::new();
    engine
        .compile_artifact_with_pipeline(&lowered.artifact, pipeline)
        .map_err(|err| anyhow::anyhow!("JIT compile failed: {err}"))?;

    let entrypoint_info = lowered
        .resolution
        .items
        .iter()
        .find(|item| {
            item.name == entrypoint
                && (item.kind == ItemKind::Function || item.kind == ItemKind::Test)
        })
        .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;

    let signature = lowered
        .typed
        .function_signatures
        .get(&entrypoint_info.id)
        .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;

    if !signature.params.is_empty() {
        return Err(anyhow::anyhow!(
            "Entrypoint `{entrypoint}` must take no parameters"
        ));
    }

    let return_info = lowered
        .typed
        .types
        .get(signature.return_type)
        .ok_or_else(|| anyhow::anyhow!("Missing return type for `{entrypoint}`"))?;

    let ptr = unsafe { engine.entrypoint_ptr(entrypoint) }
        .map_err(|err| anyhow::anyhow!("Entrypoint lookup failed: {err}"))?;
    if ptr.is_null() {
        return Err(anyhow::anyhow!(
            "Entrypoint `{entrypoint}` returned null pointer"
        ));
    }

    let output = engine.with_runtime(|_, _| JitCallable::execute_and_format(ptr, return_info));

    Ok(output)
}
