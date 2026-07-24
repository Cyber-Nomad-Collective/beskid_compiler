//! `beskid analyze` — run builtin semantic rules and print diagnostics.

use anyhow::Result;
use beskid_analysis::services::{self, FrontEndOptions, PrepareOptions};
use clap::Args;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_tools::pipeline::{
    CliResolveOptions, resolve_input_with_cli_pipeline, tui::format_severity_summary, tui::severity_command_summary,
};
use beskid_tools::tui::shell::runtime::RuntimeOp;

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
}

/// Resolve the project, analyze the entry source, and print diagnostics (or "No diagnostics.").
pub fn execute(args: AnalyzeArgs) -> Result<()> {
    run_analyze(args)
}

/// Same as [`execute`] but forwards pipeline progress into a running `beskid hi` shell.
pub fn execute_for_hi(_msg_tx: Sender<RuntimeOp>, args: AnalyzeArgs) -> Result<()> {
    run_analyze(args)
}

fn run_analyze(args: AnalyzeArgs) -> Result<()> {
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline(CliResolveOptions::new(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.plain,
    ))?;
    let prepare_options = PrepareOptions {
        front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        ..Default::default()
    };
    let diagnostics = if resolved.compile_plan.is_some() {
        let (_, diagnostics) =
            beskid_queries::prepare_compilation_diagnostics(&resolved, prepare_options, Some(pipeline_ui.as_ref()))?;
        diagnostics
    } else if let Some(plan) = services::compile_plan_for_input_path(&resolved.source_path) {
        let project_resolved =
            services::resolved_input_from_plan(resolved.source_path.clone(), resolved.source.clone(), plan, None, None);
        let (_, diagnostics) = beskid_queries::prepare_compilation_diagnostics(
            &project_resolved,
            prepare_options,
            Some(pipeline_ui.as_ref()),
        )?;
        diagnostics
    } else {
        services::analyze_program(&resolved.source_path, &resolved.source)?
    };
    let counts = pipeline_ui.report_semantic_diagnostics(&diagnostics);
    let severity_line = format_severity_summary(counts);
    let summary = severity_command_summary("Analyze", format!("Analyze complete ({severity_line})"), counts);
    pipeline_ui.finish_session_with_summary(format!("Analyze complete ({severity_line})"), Some(summary));
    Ok(())
}
