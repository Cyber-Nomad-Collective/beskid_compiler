use std::path::Path;

use anyhow::Result;
use beskid_analysis::AnalysisOptions;
use beskid_analysis::hir::HirProgram;
use beskid_analysis::resolve::Resolution;
use beskid_analysis::services::{
    SemanticDiagnosticsError, compile_plan_for_input_path, lower_normalize_resolve_type_spanned,
    require_no_semantic_errors, semantic_rule_diagnostics_for_program,
};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeResult;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{CODEGEN_CLIF, LOWER, LOWER_READY, PARSE, SEMANTIC, SEMANTIC_SNAPSHOT},
};

use crate::{CodegenArtifact, codegen_errors_to_diagnostics, lower_program};

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

/// End-to-end lowering from source: parse, optional semantic checks, HIR lowering, then Cranelift codegen.
///
/// When `pipeline` is set, emits the same phase ids documented on the crate (`parse`, semantic phases,
/// [`beskid_pipeline::phases::LOWER_READY`], `lower`, [`beskid_pipeline::phases::CODEGEN_CLIF`]).
pub fn lower_source_with_pipeline(
    path: &Path,
    source: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let source_name = path.display().to_string();
    let program = observe_phase_result(pipeline, PARSE, || {
        beskid_analysis::services::parse_program_with_source_name(&source_name, source)
    })?;
    let compile_plan = compile_plan_for_input_path(path);
    let generated = beskid_analysis::mod_host::run_through_generate(
        program,
        &beskid_analysis::mod_host::ModHostInput {
            compile_plan: compile_plan.as_ref(),
            source_name: &source_name,
            source,
            pipeline,
        },
    )?;
    let mut program = generated.program;

    if with_diagnostics {
        observe_phase_result(pipeline, SEMANTIC, || {
            let diagnostics = semantic_rule_diagnostics_for_program(
                &program.node,
                source_name.clone(),
                source,
                AnalysisOptions::default(),
            );
            require_no_semantic_errors(&diagnostics).map_err(anyhow::Error::from)
        })?;
        observe_phase(pipeline, SEMANTIC_SNAPSHOT, || {});
    }
    program =
        beskid_analysis::mod_host::run_analyze_rewrite(program, &generated.session, pipeline)?;

    observe_phase(pipeline, LOWER_READY, || {});

    let (hir, resolution, typed) = observe_phase_result(
        pipeline,
        LOWER,
        || -> Result<(Spanned<HirProgram>, Resolution, TypeResult), anyhow::Error> {
            lower_normalize_resolve_type_spanned(&program).map_err(anyhow::Error::new)
        },
    )?;

    let artifact = observe_phase_result(pipeline, CODEGEN_CLIF, || {
        lower_program(&hir, &resolution, &typed).map_err(|errors| {
            let diagnostics =
                codegen_errors_to_diagnostics(&path.display().to_string(), source, &errors);
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
