//! `beskid run` — AOT-compile the resolved program and execute it in a subprocess.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use crate::runtime_profile::CliRuntimeProfile;
use anyhow::Result;
use beskid_aot::{
    AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, build,
    default_runtime_strategy, run_linked_executable,
};
use beskid_codegen::services::lower_from_front_end;
use beskid_engine::link_libraries::{apply_link_libraries, link_libraries_for_artifact};
use beskid_pipeline::PipelineObserver;
use beskid_tools::PipelineProgressKind;
use beskid_tools::pipeline::tui::CommandSummary;
use beskid_tools::session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
use clap::Args;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The input Beskid file to AOT-compile and execute
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Entrypoint function name
    #[arg(long, default_value = "Main")]
    pub entrypoint: String,

    /// Disable animated progress and graph output
    #[arg(long)]
    pub plain: bool,

    /// Runtime link profile: `std` links `beskid_host`; `minimal` is language runtime only
    #[arg(long, value_enum, default_value_t = CliRuntimeProfile::Std)]
    pub runtime_profile: CliRuntimeProfile,
}

/// Resolve, AOT-link, and run `args.entrypoint` in a subprocess with pipeline progress on stderr when enabled.
pub fn execute(args: RunArgs) -> Result<()> {
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
    let source_name = resolved.source_path.display().to_string();
    let lowered = lower_from_front_end(
        &source_name,
        &resolved.source,
        front,
        Some(&args.entrypoint),
        Some(session.observer()),
    )?;
    let artifact = lowered.artifact;

    let temp_dir = std::env::temp_dir().join(format!(
        "beskid_run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&temp_dir).map_err(|err| {
        anyhow::anyhow!(
            "failed to create run output directory {}: {err}",
            temp_dir.display()
        )
    })?;

    let target = beskid_aot::target::detect_target(None)?;
    let exe_path = temp_dir.join(beskid_aot::target::output_filename(
        "beskid_run",
        BuildOutputKind::Exe,
        &target,
    ));

    let runtime = default_runtime_strategy(
        BuildProfile::Debug,
        None,
        args.runtime_profile.into(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;

    let link_inputs = link_libraries_for_artifact(&artifact, resolved.compile_plan.as_ref());
    let pipeline_arc: Arc<dyn PipelineObserver> = session.pipeline_arc();
    let mut build_request = AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::Exe,
        output_path: exe_path.clone(),
        object_path: None,
        target_triple: None,
        profile: BuildProfile::Debug,
        entrypoint: args.entrypoint.clone(),
        export_policy: ExportPolicy::PublicOnly,
        link_mode: LinkMode::Auto,
        runtime,
        runtime_link_profile: args.runtime_profile.into(),
        verbose_link: false,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: Some(pipeline_arc),
    };
    apply_link_libraries(&mut build_request, link_inputs);

    let build_result = build(build_request).map_err(|err| anyhow::anyhow!("{err}"))?;
    let exe_path = build_result.final_path.unwrap_or(exe_path);

    let run_result = run_linked_executable(&exe_path).map_err(|err| anyhow::anyhow!("{err}"))?;
    session
        .pipeline()
        .finish_session_with_summary(
            "Run complete",
            Some(
                CommandSummary::plain("Run", "Run complete")
                    .with_stat("exit", run_result.exit_code.to_string()),
            ),
        );

    if !run_result.stdout.is_empty() {
        io::stdout().write_all(&run_result.stdout)?;
    }
    if !run_result.stderr.is_empty() {
        io::stderr().write_all(&run_result.stderr)?;
    }

    let _ = std::fs::remove_dir_all(temp_dir);

    if run_result.exit_code != 0 {
        std::process::exit(run_result.exit_code);
    }

    Ok(())
}
