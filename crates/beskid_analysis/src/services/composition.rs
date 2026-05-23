use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::diagnostics::make_diagnostic;
use crate::analysis::SemanticDiagnostic;
use crate::composition::baseline::lib_project_rejects_launch;
use crate::composition::{CompositionInput, CompositionIssue, CompositionResult};
use crate::mod_host::{ModHostGenerateResult, ModHostInput, run_through_generate};
use crate::projects::CompilePlan;
use crate::syntax::{Program, SpanInfo, Spanned};

pub fn is_mod_compile_plan(plan: Option<&CompilePlan>) -> bool {
    plan.is_some_and(|compile_plan| compile_plan.target.name == "__mod__")
}

/// Run mod-host generate (including macro expansion) for composition analysis parity with the build spine.
pub fn prepare_program_for_composition(
    program: Spanned<Program>,
    compile_plan: Option<&CompilePlan>,
    source_name: &str,
    source: &str,
) -> anyhow::Result<ModHostGenerateResult> {
    run_through_generate(
        program,
        &ModHostInput {
            compile_plan,
            source_name,
            source,
            pipeline: None,
            invoker: None,
        },
    )
}

pub fn resolve_program_composition(
    program: &Spanned<Program>,
    compile_plan: Option<&CompilePlan>,
) -> CompositionResult {
    let ast_program: Spanned<crate::hir::AstProgram> = program.clone().into();
    let hir_program = crate::hir::lower_program(&ast_program);
    crate::composition::resolve_composition(CompositionInput {
        program: &hir_program,
        is_mod_project: is_mod_compile_plan(compile_plan),
    })
}

pub fn composition_diagnostics_for_program(
    program: &Spanned<Program>,
    compile_plan: Option<&CompilePlan>,
    source_name: &str,
    source: &str,
) -> anyhow::Result<Vec<SemanticDiagnostic>> {
    let generated = prepare_program_for_composition(program.clone(), compile_plan, source_name, source)?;
    let composition_result = resolve_program_composition(&generated.program, compile_plan);
    Ok(composition_result_to_diagnostics(
        &composition_result,
        generated.program.span,
        source_name,
        source,
        compile_plan,
    ))
}

pub fn composition_result_to_diagnostics(
    composition: &CompositionResult,
    program_span: SpanInfo,
    source_name: &str,
    source: &str,
    plan: Option<&CompilePlan>,
) -> Vec<SemanticDiagnostic> {
    let mut diagnostics = composition
        .issues
        .iter()
        .map(|issue| composition_issue_to_diagnostic(issue, program_span, source_name, source))
        .collect::<Vec<_>>();

    if plan.is_some_and(|compile_plan| {
        lib_project_rejects_launch(compile_plan.target.kind) && !composition.snapshot.launched_host.is_empty()
    })
    {
        let kind = SemanticIssueKind::CompositionLaunchInLibProject;
        diagnostics.push(make_diagnostic(
            source_name,
            source,
            composition.snapshot.launch_span.unwrap_or(program_span),
            kind.message(),
            kind.label(),
            kind.help(),
            Some(kind.code().to_string()),
            kind.severity(),
        ));
    }

    diagnostics
}

fn composition_issue_to_diagnostic(
    issue: &CompositionIssue,
    program_span: SpanInfo,
    source_name: &str,
    source: &str,
) -> SemanticDiagnostic {
    if let Some((span, mapped_kind)) = crate::composition::diagnostics::to_semantic_issue(issue) {
        return make_diagnostic(
            source_name,
            source,
            span,
            mapped_kind.message(),
            mapped_kind.label(),
            mapped_kind.help(),
            Some(mapped_kind.code().to_string()),
            mapped_kind.severity(),
        );
    }

    let (span, message, label, help) = match issue {
        CompositionIssue::UnknownRegistrationId {
            registration_id,
            span,
        } => (
            span.unwrap_or(program_span),
            format!(
                "composition produced unknown registration id `{registration_id}` while building dependency graph"
            ),
            "invalid composition registration id",
            Some("check host/scope overrides and registration dependencies".to_string()),
        ),
        other => (
            program_span,
            format!("unmapped composition issue: {other:?}"),
            "unmapped composition issue",
            Some("report this issue variant to the compiler team".to_string()),
        ),
    };
    make_diagnostic(
        source_name,
        source,
        span,
        message,
        label,
        help,
        Some(crate::composition::diagnostics::composition_issue_code(issue).to_string()),
        crate::analysis::Severity::Error,
    )
}
