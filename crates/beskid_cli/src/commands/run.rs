//! `beskid run` — AOT-compile the resolved program and execute it in a subprocess.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::commands::syntax_codegen::lower_prepared_entrypoint;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_aot::{
    AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, build, default_runtime_strategy,
    run_linked_executable,
};
use beskid_engine::link_libraries::{apply_link_libraries, link_libraries_for_artifact};
use beskid_pipeline::PipelineObserver;
use beskid_tools::PipelineProgressKind;
use beskid_tools::pipeline::tui::CommandSummary;
use beskid_tools::session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
use beskid_tools::tui::shell::runtime::RuntimeOp;
use crate::commands::step_progress::{log_step, log_step_with_duration};
use clap::Args;
use std::sync::mpsc::Sender;

#[derive(Args, Debug, Clone)]
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
}

/// Resolve, AOT-link, and run `args.entrypoint` in a subprocess.
pub fn execute(args: RunArgs) -> Result<()> {
    run_build_and_execute(args, None)
}

/// Same as [`execute`] but forwards pipeline progress into an attached shell sink.
pub fn execute_for_hi(msg_tx: Sender<RuntimeOp>, args: RunArgs) -> Result<()> {
    run_build_and_execute(args, Some(msg_tx))
}

fn run_build_and_execute(args: RunArgs, hi_tx: Option<Sender<RuntimeOp>>) -> Result<()> {
    let total_steps = 4usize;

    log_step(1, total_steps, "resolve", "loading project inputs");
    let resolve_started = Instant::now();
    let resolve_args = ResolveInputArgs {
        input: args.input.as_ref(),
        project: args.project.project.as_ref(),
        target: args.project.target.as_deref(),
        workspace_member: args.project.workspace_member.as_deref(),
        frozen: args.lockfile.frozen,
        locked: args.lockfile.locked,
    };
    let (session, resolved) = match hi_tx {
        None => CommandSession::open_and_resolve(args.plain, PipelineProgressKind::PrepareAndRun, &resolve_args)?,
        Some(tx) => {
            let session = CommandSession::with_attached_pipeline(tx, PipelineProgressKind::PrepareAndRun);
            let resolved = session.resolve_input(&resolve_args)?;
            (session, resolved)
        }
    };
    log_step_with_duration(1, total_steps, "resolve", resolve_started.elapsed());

    let prepare_started = Instant::now();
    log_step(2, total_steps, "prepare", "semantic checks and pipeline readiness");
    let prepared = session.executable_gate_prepared(&resolved, SemanticGateOptions::default())?;
    let front = prepared.into_executable()?;
    log_step_with_duration(2, total_steps, "prepare", prepare_started.elapsed());

    log_step(3, total_steps, "lower", "compiling and linking");
    let lower_started = Instant::now();
    let artifact = lower_prepared_entrypoint(&front, &args.entrypoint, None, Some(session.observer()))?;

    let temp_dir = std::env::temp_dir().join(format!(
        "beskid_run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|err| anyhow::anyhow!("failed to create run output directory {}: {err}", temp_dir.display()))?;

    let target = beskid_aot::target::detect_target(None)?;
    let exe_path = temp_dir.join(beskid_aot::target::output_filename("beskid_run", BuildOutputKind::Exe, &target));

    let runtime_profile = match std::env::var("BESKID_RUNTIME_KIT_PROFILE") {
        Ok(value) => match beskid_abi::runtime_kit::BuildProfile::parse(&value)? {
            beskid_abi::runtime_kit::BuildProfile::Debug => BuildProfile::Debug,
            beskid_abi::runtime_kit::BuildProfile::Release => BuildProfile::Release,
        },
        Err(std::env::VarError::NotPresent) => BuildProfile::Debug,
        Err(error) => return Err(anyhow::anyhow!("invalid BESKID_RUNTIME_KIT_PROFILE: {error}")),
    };
    let runtime = default_runtime_strategy(runtime_profile, None)?;

    let link_inputs = link_libraries_for_artifact(&artifact, resolved.compile_plan.as_ref());
    let pipeline_arc: Arc<dyn PipelineObserver> = session.pipeline_arc();
    let mut build_request = AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::Exe,
        output_path: exe_path.clone(),
        object_path: None,
        target_triple: None,
        profile: runtime_profile,
        entrypoint: args.entrypoint.clone(),
        export_policy: ExportPolicy::PublicOnly,
        link_mode: LinkMode::Auto,
        runtime: Some(runtime),
        verbose_link: false,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: Some(pipeline_arc),
    };
    apply_link_libraries(&mut build_request, link_inputs);

    let build_result = build(build_request)?;
    let exe_path = build_result.final_path.unwrap_or(exe_path);
    let compile_duration = lower_started.elapsed();
    log_step_with_duration(3, total_steps, "lower", compile_duration);

    log_step(4, total_steps, "run", "executing compiled artifact");
    let run_started = Instant::now();
    let run_result = run_linked_executable(&exe_path)?;
    session.pipeline().finish_session_with_summary(
        "Run complete",
        Some(CommandSummary::plain("Run", "Run complete").with_stat("exit", run_result.exit_code.to_string())),
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

    log_step_with_duration(4, total_steps, "run", run_started.elapsed());

    Ok(())
}
