//! `beskid clif` — lower resolved Beskid source to CLIF and print the IR.

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_codegen::render_clif;
use beskid_pipeline::{observe_phase_result, phases::CODEGEN_CLIF, PipelineObserver};
use beskid_tools::pipeline::tui::CommandSummary;
use beskid_tools::session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
use beskid_tools::PipelineProgressKind;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ClifArgs {
    /// The input Beskid file to lower into CLIF
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Disable animated progress during resolve and lowering
    #[arg(long)]
    pub plain: bool,
}

/// Resolve and lower the program, then print rendered CLIF for the default entry.
pub fn execute(args: ClifArgs) -> Result<()> {
    let resolve_args = ResolveInputArgs {
        input: args.input.as_ref(),
        project: args.project.project.as_ref(),
        target: args.project.target.as_deref(),
        workspace_member: args.project.workspace_member.as_deref(),
        frozen: args.lockfile.frozen,
        locked: args.lockfile.locked,
    };
    let (session, resolved) = CommandSession::open_and_resolve(
        args.plain,
        PipelineProgressKind::PrepareAndRun,
        &resolve_args,
    )?;
    let prepared = session.executable_gate_prepared(&resolved, SemanticGateOptions::default())?;
    let front = prepared.into_executable()?;
    let artifact = lower_prepared_clif_with_pipeline(&front, "Main", Some(session.observer()))?;
    session.pipeline().finish_session_with_summary(
        "CLIF ready",
        Some(CommandSummary::plain("CLIF", "CLIF ready")),
    );
    print!("{}", render_clif(&artifact));

    Ok(())
}

/// Lower the prepared expanded-syntax assembly through the sole HIR-free codegen boundary.
///
/// Semantic diagnostics are complete before this function is called; the returned artifact has
/// the same command-facing CLIF contract as the retired `lower_from_front_end` path.
fn lower_prepared_clif(
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
) -> Result<beskid_codegen::CodegenArtifact> {
    let target = beskid_engine::host_runtime_target()
        .map_err(|error| anyhow::anyhow!("native ABI-v5 target unavailable: {error}"))?;
    beskid_engine::services::lower_prepared_syntax_entrypoint(front, entrypoint, target)
}

fn lower_prepared_clif_with_pipeline(
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_codegen::CodegenArtifact> {
    observe_phase_result(pipeline, CODEGEN_CLIF, || {
        lower_prepared_clif(front, entrypoint)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::services::{
        resolved_input_from_plan, synthetic_compile_plan_for_source, FrontEndOptions, ResolvedInput,
    };
    use beskid_queries::compile_front_end_from_resolved_input;

    #[test]
    fn prepared_clif_emits_reachable_syntax_items_without_hir_lowering() {
        let directory = std::env::temp_dir().join(format!(
            "beskid_cli_clif_syntax_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).expect("create test project");
        let path = directory.join("Main.bd");
        let source = "i32 Echo(i32 value) { return value; } i32 Main() { return Echo(41); }";
        std::fs::write(&path, source).expect("write source");
        let plan = synthetic_compile_plan_for_source(&path);
        let resolved: ResolvedInput =
            resolved_input_from_plan(path.clone(), source.into(), plan, None, None);
        let front = compile_front_end_from_resolved_input(
            &resolved,
            FrontEndOptions {
                with_semantic_diagnostics: false,
                ..Default::default()
            },
            None,
        )
        .expect("prepare frontend");

        let artifact = lower_prepared_clif(&front, "Main").expect("syntax CLIF");

        assert_eq!(artifact.functions.len(), 2);
        assert!(
            artifact
                .functions
                .iter()
                .any(|function| function.name.starts_with("Echo#syntax_")),
            "emitted functions: {:?}",
            artifact
                .functions
                .iter()
                .map(|function| &function.name)
                .collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(directory).expect("remove test project");
    }
}
