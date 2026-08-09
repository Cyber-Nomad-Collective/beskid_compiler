use anyhow::Result;
use beskid_analysis::services::{FrontEndOptions, ResolvedInput};
use beskid_pipeline::PipelineObserver;
use beskid_queries::with_db;

use super::SyntaxEntrypointArtifact;
use super::jit_preparation::lower_syntax_entrypoint_from_front_end;
use crate::Engine;
use crate::jit_callable::{EntryReturnKind, JitCallable};

/// Parse, lower, JIT-compile, and run `entrypoint` (no-arg function or test); returns a string summary of the return value.
pub fn run_entrypoint(source_path: &std::path::Path, source: &str, entrypoint: &str) -> Result<String> {
    run_entrypoint_with_pipeline(source_path, source, entrypoint, None)
}

/// Same as [`run_entrypoint`] with optional pipeline observation for codegen and JIT phases.
pub fn run_entrypoint_with_pipeline(
    source_path: &std::path::Path,
    source: &str,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    let source_path = beskid_codegen::materialize_source_path_for_lowering(source_path, source)?;
    let compile_plan = beskid_analysis::services::compile_plan_for_input_path(&source_path)
        .or_else(|| Some(beskid_analysis::services::synthetic_compile_plan_for_source(&source_path)));
    let resolved = ResolvedInput {
        source_path,
        source: source.to_string(),
        compile_plan,
        prepared_workspace: None,
        workspace_summary: None,
        assembly: None,
    };
    let front = beskid_queries::compile_front_end_from_resolved_input(&resolved, FrontEndOptions::default(), pipeline)?;
    run_entrypoint_from_front_end_with_pipeline(
        &front,
        &resolved.source_path.display().to_string(),
        &resolved.source,
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
    run_entrypoint_from_front_end_with_engine(&mut engine, front, source_name, source, entrypoint, pipeline)
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
    let syntax_entrypoint = with_db(|db| {
        lower_syntax_entrypoint_from_front_end(db, front, entrypoint, engine.target_metadata().clone(), pipeline)
    })?;

    // `source_name` and `source` remain part of the public API for compatibility with callers
    // that share this service with diagnostic paths. The production handoff below exclusively
    // consumes the already prepared expanded syntax assembly.
    let _ = (source_name, source);
    run_syntax_jitted_entrypoint(engine, &syntax_entrypoint, entrypoint, pipeline)
}

fn run_syntax_jitted_entrypoint(
    engine: &mut Engine,
    entrypoint_artifact: &SyntaxEntrypointArtifact,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<String> {
    engine
        .compile_artifact_with_pipeline(&entrypoint_artifact.artifact, pipeline)
        .map_err(|err| anyhow::anyhow!("JIT compile failed: {err}"))?;

    let ptr = unsafe { engine.entrypoint_ptr(&entrypoint_artifact.symbol) }
        .map_err(|err| anyhow::anyhow!("Entrypoint lookup failed: {err}"))?;
    if ptr.is_null() {
        return Err(anyhow::anyhow!("Entrypoint `{entrypoint}` returned null pointer"));
    }

    let return_kind = EntryReturnKind::from_semantic_type(entrypoint_artifact.return_type);
    // JIT'd entrypoints execute on this thread and may allocate through the runtime (string
    // interpolation, gc roots, collections). AOT-linked executables install a main-thread heap
    // and runtime root via `beskid_runtime_link_anchor`; the in-process JIT path has no linker
    // anchor, so enable the same lazy main-thread bootstrap here. The runtime installs a default
    // heap/root on the first `with_current_root` call instead of aborting with "no active runtime
    // root".
    beskid_runtime::gc::enable_aot_main_bootstrap();
    if beskid_host::beskid_host_register_all() != 0 {
        return Err(anyhow::anyhow!("failed to register JIT host dispatch handlers"));
    }
    let output = JitCallable::execute_as_i64(ptr, return_kind);
    Ok(JitCallable::format_i64_result(output, return_kind))
}
