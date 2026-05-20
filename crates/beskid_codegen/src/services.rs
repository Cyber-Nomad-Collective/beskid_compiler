use std::path::Path;

use anyhow::Result;
use beskid_analysis::hir::HirProgram;
use beskid_analysis::resolve::Resolution;
use beskid_analysis::services::{
    FrontEndOptions, compile_front_end_from_resolved_input, compile_front_end_with_pipeline,
};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeResult;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::CODEGEN_CLIF};

use crate::{CodegenArtifact, codegen_errors_to_diagnostics, lower_program};
use beskid_analysis::services::{ResolvedInput, SemanticDiagnosticsError};

/// Fully lowered program: typed HIR plus the Cranelift artifact from [`lower_source`] /
/// [`lower_source_with_pipeline`].
pub struct LoweredProgram {
    pub hir: Spanned<HirProgram>,
    pub resolution: Resolution,
    pub typed: TypeResult,
    pub artifact: CodegenArtifact,
}

/// Parse, optionally run semantic diagnostics, lower to HIR, and codegen to CLIF without pipeline hooks.
pub fn lower_source(path: &Path, source: &str, with_diagnostics: bool) -> Result<LoweredProgram> {
    lower_source_with_pipeline(path, source, with_diagnostics, None)
}

/// End-to-end lowering from source via the shared analysis front-end spine.
pub fn lower_source_with_pipeline(
    path: &Path,
    source: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let mut options = FrontEndOptions::default();
    options.with_semantic_diagnostics = with_diagnostics;

    if let Some(plan) = beskid_analysis::services::compile_plan_for_input_path(path) {
        let front =
            compile_front_end_with_pipeline(path, source, Some(&plan), None, options, pipeline)?;
        return lower_from_front_end(&path.display().to_string(), source, front, pipeline);
    }

    lower_source_single_unit_legacy(path, source, with_diagnostics, pipeline)
}

fn lower_source_single_unit_legacy(
    path: &Path,
    source: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    use beskid_analysis::AnalysisOptions;
    use beskid_analysis::mod_host::{ModHostInput, run_analyze_rewrite, run_through_generate};
    use beskid_analysis::services::{
        lower_normalize_resolve_type_spanned, parse_program_with_source_name,
        require_no_semantic_errors, semantic_rule_diagnostics_for_program,
    };
    use beskid_pipeline::{observe_phase, observe_phase_result, phases};

    let source_name = path.display().to_string();
    let program = observe_phase_result(pipeline, phases::PARSE, || {
        parse_program_with_source_name(&source_name, source)
    })?;
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: None,
            source_name: &source_name,
            source,
            pipeline,
        },
    )?;
    let mut program = generated.program;
    if with_diagnostics {
        observe_phase_result(pipeline, phases::SEMANTIC, || {
            let diagnostics = semantic_rule_diagnostics_for_program(
                &program.node,
                source_name.clone(),
                source,
                AnalysisOptions::default(),
            );
            require_no_semantic_errors(&diagnostics).map_err(anyhow::Error::from)
        })?;
        observe_phase(pipeline, phases::SEMANTIC_SNAPSHOT, || {});
    }
    program = run_analyze_rewrite(program, &generated.session, pipeline)?;
    observe_phase(pipeline, phases::LOWER_READY, || {});
    let (hir, resolution, typed) = observe_phase_result(pipeline, phases::LOWER, || {
        lower_normalize_resolve_type_spanned(&program).map_err(anyhow::Error::from)
    })?;
    let artifact = observe_phase_result(pipeline, CODEGEN_CLIF, || {
        lower_program(&hir, &resolution, &typed).map_err(|errors| {
            let diagnostics = codegen_errors_to_diagnostics(&source_name, source, &errors);
            anyhow::Error::new(SemanticDiagnosticsError::from_diagnostics(diagnostics))
        })
    })?;
    Ok(LoweredProgram {
        hir,
        resolution,
        typed,
        artifact,
    })
}

/// Lower using a fully resolved CLI input (includes materialized assembly when available).
pub fn lower_resolved_input_with_pipeline(
    resolved: &ResolvedInput,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let mut options = FrontEndOptions::default();
    options.with_semantic_diagnostics = with_diagnostics;

    let front = compile_front_end_from_resolved_input(resolved, options, pipeline)?;

    lower_from_front_end(
        &resolved.source_path.display().to_string(),
        &resolved.source,
        front,
        pipeline,
    )
}

fn lower_from_front_end(
    source_name: &str,
    source: &str,
    front: beskid_analysis::services::FrontEndTypedResult,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let artifact = observe_phase_result(pipeline, CODEGEN_CLIF, || {
        lower_program(&front.hir, &front.resolution, &front.typed).map_err(|errors| {
            let diagnostics = codegen_errors_to_diagnostics(source_name, source, &errors);
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
