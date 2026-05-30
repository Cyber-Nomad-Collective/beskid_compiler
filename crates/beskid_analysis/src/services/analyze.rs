//! High-level analysis: parse source, run semantic rules, and filter diagnostics using project context.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::analysis::SemanticDiagnostic;
use crate::compilation_context::CompilationContext;
use crate::projects::CompilePlan;

use super::document::resolve_program_with_assembly;
use super::input::{AnalyzeInProjectOptions, ResolvedInput};
use super::parse::parse_program_with_source_name;
use super::prepare::{PrepareMode, PrepareOptions, prepare_compilation_diagnostics, resolved_input_from_plan};
use super::front_end::FrontEndOptions;

pub fn analyze_program(path: &Path, source: &str) -> Result<Vec<SemanticDiagnostic>> {
    analyze_program_with_options(path, source, crate::AnalysisOptions::default())
}

pub fn analyze_program_with_options(
    path: &Path,
    source: &str,
    options: crate::AnalysisOptions,
) -> Result<Vec<SemanticDiagnostic>> {
    analyze_program_with_options_and_plan(path, source, options, None)
}

fn analyze_program_with_options_and_plan(
    path: &Path,
    source: &str,
    _options: crate::AnalysisOptions,
    compile_plan: Option<&CompilePlan>,
) -> Result<Vec<SemanticDiagnostic>> {
    if let Some((span, keyword)) = crate::parsing::reserved_keywords::find_reserved_keyword(source)
    {
        use crate::analysis::diagnostic_kinds::SemanticIssueKind;
        use crate::analysis::diagnostics::make_diagnostic;
        use crate::parsing::reserved_keywords::ReservedKeyword;

        let (code, message) = match keyword {
            ReservedKeyword::Async => (
                SemanticIssueKind::AsyncKeywordReserved.code(),
                SemanticIssueKind::AsyncKeywordReserved.message(),
            ),
            ReservedKeyword::Await => (
                SemanticIssueKind::AwaitKeywordReserved.code(),
                SemanticIssueKind::AwaitKeywordReserved.message(),
            ),
        };
        let label = match keyword {
            ReservedKeyword::Async => SemanticIssueKind::AsyncKeywordReserved.label(),
            ReservedKeyword::Await => SemanticIssueKind::AwaitKeywordReserved.label(),
        };
        return Ok(vec![make_diagnostic(
            &path.display().to_string(),
            source,
            span,
            message,
            label,
            None,
            Some(code.to_string()),
            crate::analysis::Severity::Error,
        )]);
    }

    if compile_plan.is_none() {
        let source_name = path.display().to_string();
        let program = parse_program_with_source_name(&source_name, source)?;
        return Ok(super::semantic::semantic_rule_diagnostics_for_program(
            &program.node,
            source_name,
            source,
            crate::AnalysisOptions::default(),
        ));
    }

    let plan = compile_plan.expect("checked above");
    let resolved = resolved_input_from_plan(
        path.to_path_buf(),
        source.to_string(),
        plan.clone(),
        None,
        None,
    );

    let prepare_options = PrepareOptions {
        mode: PrepareMode::DiagnosticsOnly,
        front_end: FrontEndOptions {
            with_semantic_diagnostics: true,
            ..Default::default()
        },
    };

    let (_prepared, diagnostics) =
        prepare_compilation_diagnostics(&resolved, prepare_options, None)?;

    Ok(diagnostics)
}

pub fn analyze_file_in_project(path: &Path) -> Result<Vec<SemanticDiagnostic>> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    analyze_source_in_project(path, &source)
}

pub fn analyze_source_in_project(path: &Path, source: &str) -> Result<Vec<SemanticDiagnostic>> {
    analyze_source_in_project_with_options(path, source, AnalyzeInProjectOptions::default())
}

/// Run project-aware analysis using a pre-resolved [`CompilationContext`].
pub fn analyze_source_with_compilation_context(
    path: &Path,
    source: &str,
    ctx: &mut CompilationContext,
) -> Result<Vec<SemanticDiagnostic>> {
    let Some(plan) = ctx.compile_plan.clone() else {
        return analyze_program(path, source);
    };

    let assembly = ctx.assembly_for_entry(path, source).cloned();
    let resolved = resolved_input_from_plan(
        path.to_path_buf(),
        source.to_string(),
        plan.clone(),
        ctx.prepared_workspace.clone(),
        assembly,
    );

    let prepare_options = PrepareOptions {
        mode: PrepareMode::DiagnosticsOnly,
        front_end: FrontEndOptions {
            with_semantic_diagnostics: true,
            module_level_meta_items_allowed: Some(ctx.module_level_meta_items_allowed()),
            ..Default::default()
        },
    };

    let (_prepared, mut diagnostics) =
        prepare_compilation_diagnostics(&resolved, prepare_options, None)?;

    if let Some(assembly) = ctx.assembly_for_entry(path, source) {
        let source_name = path.display().to_string();
        if let Ok(program) = parse_program_with_source_name(&source_name, source)
            && resolve_program_with_assembly(&program, assembly, path).is_some()
        {
            diagnostics.retain(|diag| {
                !matches!(diag.code.as_deref(), Some("E1105") | Some("E1108"))
            });
        }
    }

    if is_non_entry_project_file(path, Some(&plan)) {
        diagnostics.retain(|diagnostic| diagnostic.code.as_deref() == Some("parse"));
        return Ok(diagnostics);
    }

    Ok(diagnostics)
}

pub fn analyze_source_in_project_with_options(
    path: &Path,
    source: &str,
    options: AnalyzeInProjectOptions<'_>,
) -> Result<Vec<SemanticDiagnostic>> {
    let mut graph_opts = options.project_graph.clone();
    if graph_opts.workspace_member_for_meta_default.is_none()
        && let Some(member) = options.workspace_member
    {
        graph_opts.workspace_member_for_meta_default = Some(member.to_string());
    }

    match CompilationContext::try_for_analysis_path_with_graph_options(
        path,
        options.workspace_member,
        graph_opts,
    ) {
        Some(mut ctx) => analyze_source_with_compilation_context(path, source, &mut ctx),
        None => analyze_program(path, source),
    }
}

pub fn compile_plan_for_input_path(path: &Path) -> Option<CompilePlan> {
    CompilationContext::try_for_analysis_path(path, None).and_then(|c| c.compile_plan)
}

pub fn compile_plan_for_input_path_with_member(
    path: &Path,
    workspace_member: Option<&str>,
) -> Option<CompilePlan> {
    CompilationContext::try_for_analysis_path(path, workspace_member).and_then(|c| c.compile_plan)
}

fn is_non_entry_project_file(path: &Path, plan: Option<&CompilePlan>) -> bool {
    let Some(plan) = plan else {
        return false;
    };
    let entry_path = plan.source_root.join(&plan.target.entry);
    match (path.canonicalize(), entry_path.canonicalize()) {
        (Ok(path), Ok(entry)) => path != entry,
        _ => path != entry_path,
    }
}

/// Build a [`ResolvedInput`] from compilation context for shared prepare spine callers.
pub fn resolved_input_from_context(
    path: &Path,
    source: &str,
    ctx: &CompilationContext,
) -> Option<ResolvedInput> {
    let plan = ctx.compile_plan.clone()?;
    Some(resolved_input_from_plan(
        path.to_path_buf(),
        source.to_string(),
        plan,
        ctx.prepared_workspace.clone(),
        ctx.assembly.clone(),
    ))
}
