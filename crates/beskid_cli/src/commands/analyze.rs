//! `beskid analyze` — run builtin semantic rules and print diagnostics.

use anyhow::Result;
use beskid_analysis::services;
use clap::Args;
use std::path::PathBuf;

use crate::pipeline_ui::{resolve_input_with_cli_pipeline, tui::format_severity_summary};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

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
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.plain,
    )?;
    pipeline_ui.show_build_graph(&resolved);
    let counts = pipeline_ui.report_semantic_diagnostics(&services::analyze_source_in_project(
        &resolved.source_path,
        &resolved.source,
    )?);
    pipeline_ui.finish_session(format!(
        "Analyze complete ({})",
        format_severity_summary(counts)
    ));
    Ok(())
}
