//! `beskid check` — run builtin semantic rules and print diagnostics.

use anyhow::Result;
use beskid_analysis::services::{self, FrontEndOptions, PrepareOptions};
use clap::Args;
use std::path::PathBuf;

use crate::machine_output::Records;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_tools::pipeline::{
    CliResolveOptions, resolve_input_with_cli_pipeline, tui::format_severity_summary, tui::severity_command_summary,
};

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// The input Beskid file to analyze
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Disable animated progress (resolve phases still emit pipeline events for tracing)
    #[arg(long)]
    pub plain: bool,

    /// Print schema-1 newline-delimited JSON records (`diagnostic`, then one `result`) on stdout
    #[arg(long)]
    pub json: bool,
}

/// Resolve the project, analyze the entry source, and print diagnostics (or "No diagnostics.").
pub fn execute(args: AnalyzeArgs) -> Result<()> {
    if args.json {
        return Records::new("check").Run(|records| run_analyze(args, Some(records)));
    }
    run_analyze(args, None).map(|_| ())
}

fn run_analyze(args: AnalyzeArgs, records: Option<&Records>) -> Result<serde_json::Value> {
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline(CliResolveOptions::new(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.WorkspaceOptions(),
        args.plain,
    ))?;
    let mod_host = super::compiler_mod::prepare_native_mod_executor(
        &resolved,
        args.lockfile.WorkspaceOptions(),
        Some(pipeline_ui.as_ref()),
    )?;
    let prepare_options = PrepareOptions {
        front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        ..PrepareOptions::with_mod_host(mod_host)
    };
    let diagnostics = if resolved.compile_plan.is_some() {
        let (_, diagnostics, _fixes) =
            beskid_queries::prepare_compilation_diagnostics(&resolved, prepare_options, Some(pipeline_ui.as_ref()))?;
        diagnostics
    } else if let Some(plan) = services::compile_plan_for_input_path(&resolved.source_path) {
        let project_resolved =
            services::resolved_input_from_plan(resolved.source_path.clone(), resolved.source.clone(), plan, None, None);
        let (_, diagnostics, _fixes) = beskid_queries::prepare_compilation_diagnostics(
            &project_resolved,
            prepare_options,
            Some(pipeline_ui.as_ref()),
        )?;
        diagnostics
    } else {
        beskid_tools::pipeline::frontend::standalone_semantic_diagnostics(
            &resolved.source_path,
            &resolved.source,
            Some(pipeline_ui.as_ref()),
        )?
    };
    let counts = pipeline_ui.report_semantic_diagnostics(&diagnostics);
    if let Some(records) = records {
        for diagnostic in &diagnostics {
            records.Diagnostic(diagnostic)?;
        }
    }
    let severity_line = format_severity_summary(counts);
    let summary = severity_command_summary("Analyze", format!("Analyze complete ({severity_line})"), counts);
    pipeline_ui.finish_session_with_summary(format!("Analyze complete ({severity_line})"), Some(summary));
    if counts.errors != 0 {
        anyhow::bail!("analysis failed with {severity_line}");
    }
    Ok(serde_json::json!({
        "source": resolved.source_path,
        "errors": counts.errors,
        "warnings": counts.warnings,
        "notes": counts.notes,
    }))
}
