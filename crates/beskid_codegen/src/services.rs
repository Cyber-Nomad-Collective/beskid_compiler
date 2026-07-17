use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use beskid_analysis::hir::HirProgram;
use beskid_analysis::resolve::Resolution;
use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, ResolvedInput, SemanticDiagnosticsError,
    SessionFingerprint, cached_executable, cached_semantic_snapshot, compile_plan_for_input_path,
    current_syntax_generation_id, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeResult;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::CODEGEN_CLIF};
use beskid_queries::compile_front_end_from_resolved_input;

use crate::{
    CodegenArtifact, codegen_errors_to_diagnostics,
    lowering::lower_program_with_assembly_for_entrypoint,
};

/// Fully lowered program: typed HIR plus the Cranelift artifact from [`lower_source`] /
/// [`lower_source_with_pipeline`].
pub struct LoweredProgram {
    pub hir: Spanned<HirProgram>,
    pub resolution: Resolution,
    pub typed: TypeResult,
    pub artifact: CodegenArtifact,
}

static SCRATCH_FILE_ID: AtomicU64 = AtomicU64::new(0);

/// Ensure `source` is readable from disk for assembly discovery (`<memory>` and missing paths).
pub fn materialize_source_path_for_lowering(path: &Path, source: &str) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let dir = std::env::temp_dir().join("beskid_codegen_scratch");
    std::fs::create_dir_all(&dir)?;
    let id = SCRATCH_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|name| !name.is_empty() && *name != "<memory>")
        .unwrap_or("main.bd");
    let file = dir.join(format!("{id}_{file_name}"));
    std::fs::write(&file, source)?;
    Ok(file)
}

/// Cranelift linker symbol for a resolved function or test item.
pub fn jit_symbol_for_item(
    resolution: &beskid_analysis::resolve::Resolution,
    item_id: beskid_analysis::resolve::ItemId,
) -> String {
    crate::lowering::function::mangle_item_function(resolution, item_id)
}

/// Parse, optionally run semantic diagnostics, lower to HIR, and codegen to CLIF without pipeline hooks.
pub fn lower_source(path: &Path, source: &str, with_diagnostics: bool) -> Result<LoweredProgram> {
    lower_source_with_pipeline(path, source, with_diagnostics, None)
}

/// Like [`lower_source`], limiting the link plan to a single entry function or test name.
pub fn lower_source_for_entrypoint(
    path: &Path,
    source: &str,
    entrypoint: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let path = materialize_source_path_for_lowering(path, source)?;
    let plan = compile_plan_for_input_path(&path)
        .unwrap_or_else(|| synthetic_compile_plan_for_source(&path));
    let resolved = resolved_input_from_plan(path, source.to_string(), plan, None, None);
    lower_resolved_entrypoint_with_pipeline(&resolved, Some(entrypoint), with_diagnostics, pipeline)
}

/// End-to-end lowering from source via the shared analysis front-end spine.
pub fn lower_source_with_pipeline(
    path: &Path,
    source: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let path = materialize_source_path_for_lowering(path, source)?;
    let plan = compile_plan_for_input_path(&path)
        .unwrap_or_else(|| synthetic_compile_plan_for_source(&path));
    let resolved = resolved_input_from_plan(path, source.to_string(), plan, None, None);
    lower_resolved_input_with_pipeline(&resolved, with_diagnostics, pipeline)
}

/// Lower using a fully resolved CLI input (includes materialized assembly when available).
pub fn lower_resolved_input_with_pipeline(
    resolved: &ResolvedInput,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    lower_resolved_entrypoint_with_pipeline(resolved, None, with_diagnostics, pipeline)
}

/// Lower from an optional prepared front-end, else session cache or full compile.
pub fn lower_from_prepared_or_cache(
    resolved: &ResolvedInput,
    front: Option<FrontEndTypedResult>,
    link_entrypoint: Option<&str>,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let front = resolve_front_end_for_lowering(resolved, front, with_diagnostics, pipeline)?;
    lower_from_front_end(
        &resolved.source_path.display().to_string(),
        &resolved.source,
        front,
        link_entrypoint,
        pipeline,
    )
}

/// Lower a single entry function or test from a resolved project input.
pub fn lower_resolved_entrypoint_with_pipeline(
    resolved: &ResolvedInput,
    link_entrypoint: Option<&str>,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    if resolved.compile_plan.is_none() {
        let entry = link_entrypoint.unwrap_or("Main");
        return lower_source_for_entrypoint(
            &resolved.source_path,
            &resolved.source,
            entry,
            with_diagnostics,
            pipeline,
        );
    }

    lower_from_prepared_or_cache(resolved, None, link_entrypoint, with_diagnostics, pipeline)
}

fn resolve_front_end_for_lowering(
    resolved: &ResolvedInput,
    front: Option<FrontEndTypedResult>,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<FrontEndTypedResult> {
    if let Some(front) = front {
        return Ok(front);
    }

    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("resolve_front_end_for_lowering requires a compile plan"))?;
    let fingerprint = SessionFingerprint::for_entry(plan, &resolved.source_path);
    if cached_front_end_is_valid(&fingerprint)
        && let Some(cached) = cached_executable(&fingerprint)
        && let Ok(owned) = Arc::try_unwrap(cached)
    {
        return Ok(owned);
    }

    let options = FrontEndOptions {
        with_semantic_diagnostics: with_diagnostics,
        ..Default::default()
    };
    compile_front_end_from_resolved_input(resolved, options, pipeline)
}

fn cached_front_end_is_valid(fingerprint: &SessionFingerprint) -> bool {
    let Some(snapshot) = cached_semantic_snapshot(fingerprint) else {
        return false;
    };
    snapshot.satisfies_minimum("executable")
        && snapshot.syntax_generation_id == current_syntax_generation_id(fingerprint)
}

/// Lower a pre-built front-end result to CLIF, optionally linking a single entrypoint.
pub fn lower_from_front_end(
    source_name: &str,
    source: &str,
    front: beskid_analysis::services::FrontEndTypedResult,
    link_entrypoint: Option<&str>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let artifact = observe_phase_result(pipeline, CODEGEN_CLIF, || {
        lower_program_with_assembly_for_entrypoint(
            &front.hir,
            &front.resolution,
            &front.typed,
            Some(&front.assembly),
            link_entrypoint,
        )
        .map_err(|errors| {
            let diagnostics = codegen_errors_to_diagnostics(
                source_name,
                source,
                &errors,
                &front.typed,
                &front.resolution,
            );
            anyhow::Error::new(SemanticDiagnosticsError::from_diagnostics(diagnostics))
        })
    })?;

    Ok(LoweredProgram {
        hir: front.hir,
        resolution: front.resolution,
        typed: front.typed,
        artifact,
    })
}

/// Serialize every lowered function in `artifact` to textual CLIF, separated by `;; Function:` headers.
pub fn render_clif(artifact: &CodegenArtifact) -> String {
    let mut out = String::new();
    for function in &artifact.functions {
        out.push_str(&format!(";; Function: {}\n", function.name));
        out.push_str(&function.function.to_string());
        out.push('\n');
    }
    out
}
