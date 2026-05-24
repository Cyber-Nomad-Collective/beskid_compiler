//! `beskid build` — AOT compile and link Beskid projects to objects, libraries, or executables.

use std::path::PathBuf;
use std::sync::Arc;

use crate::frontend;
use crate::pipeline_ui::resolve_input_with_cli_pipeline;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_analysis::projects::TargetKind;
use beskid_aot::{
    AotBuildRequest, BESKID_RUNTIME_ABI_VERSION, BuildOutputKind, BuildProfile, ExportPolicy,
    LinkMode, ProjectTargetKind, RuntimeStrategy, build, default_output_kind,
    default_runtime_strategy, resolve_entrypoint,
};
use beskid_codegen::lower_resolved_input_with_pipeline;
use beskid_engine::link_libraries::{apply_link_libraries, link_libraries_for_artifact};
use beskid_pipeline::PipelineObserver;
use clap::{Args, ValueEnum};

/// CLI-selected output artifact shape for [`BuildArgs::kind`].
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BuildKind {
    Exe,
    Shared,
    Static,
    Object,
}

/// Full set of flags for `beskid build` (output kind, profile, linker, progress).
#[derive(Args, Debug)]
pub struct BuildArgs {
    /// The input Beskid file to compile
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Entrypoint function name
    #[arg(long)]
    pub entrypoint: Option<String>,

    /// Build output kind. Defaults to Exe for App/Test targets, Shared for Lib targets.
    #[arg(long, value_enum)]
    pub kind: Option<BuildKind>,

    /// Build profile
    #[arg(long)]
    pub release: bool,

    /// Target triple override (e.g. x86_64-unknown-linux-gnu)
    #[arg(long)]
    pub target_triple: Option<String>,

    /// Final artifact output path. Defaults to <input-stem>.<ext>
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Optional object-file output path
    #[arg(long)]
    pub object_output: Option<PathBuf>,

    /// Prebuilt runtime static library (overrides the toolchain-bundled archive)
    #[arg(long)]
    pub runtime_archive: Option<PathBuf>,

    /// ABI version for `--runtime-archive` (defaults to the toolchain ABI version)
    #[arg(long)]
    pub runtime_abi_version: Option<u32>,

    /// Build in standalone mode (no Beskid runtime archive linkage)
    #[arg(long)]
    pub standalone: bool,

    /// Explicit symbols to export in shared/static artifacts
    #[arg(long = "export")]
    pub export_symbols: Vec<String>,

    /// Prefer static dependencies while linking
    #[arg(long)]
    pub prefer_static: bool,

    /// Prefer dynamic dependencies while linking
    #[arg(long)]
    pub prefer_dynamic: bool,

    /// Print linker invocations
    #[arg(long)]
    pub verbose_link: bool,

    /// Disable animated progress and graph output
    #[arg(long)]
    pub plain: bool,
}

/// Resolve, lower, emit CLIF, and run the AOT/link pipeline according to `args`.
pub fn execute(args: BuildArgs) -> Result<()> {
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.plain,
    )?;
    let obs: Option<&dyn PipelineObserver> = Some(pipeline_ui.as_ref());

    pipeline_ui.show_build_graph(&resolved);
    pipeline_ui.halt_progress_bars_for_output();

    let source = resolved.source.clone();
    let input_path = resolved.source_path.clone();
    frontend::run_semantic_analysis_gate(&input_path, &source, None, pipeline_ui.as_ref())?;
    let project_target_kind = resolved.compile_plan.as_ref().map(|plan| plan.target.kind);
    let default_output_stem = resolved
        .compile_plan
        .as_ref()
        .map(|plan| plan.target.name.clone());

    let lowered = lower_resolved_input_with_pipeline(&resolved, false, obs)?;
    let artifact = lowered.artifact;

    let output_kind = resolve_output_kind(args.kind, project_target_kind);
    let entrypoint = resolve_entrypoint(args.entrypoint)?;

    let target = beskid_aot::target::detect_target(args.target_triple.as_deref())?;
    let output = if let Some(path) = args.output {
        path
    } else {
        let stem = default_output_stem.as_deref().unwrap_or_else(|| {
            input_path
                .file_stem()
                .and_then(|part| part.to_str())
                .unwrap_or("aot_out")
        });
        let file_name = beskid_aot::target::output_filename(stem, output_kind, &target);
        let parent = input_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        parent.join(file_name)
    };

    let profile = if args.release {
        BuildProfile::Release
    } else {
        BuildProfile::Debug
    };

    let runtime = if args.standalone {
        if args.runtime_archive.is_some() {
            return Err(anyhow::anyhow!(
                "`--standalone` cannot be combined with `--runtime-archive`"
            ));
        }
        RuntimeStrategy::Standalone
    } else if let Some(path) = args.runtime_archive {
        RuntimeStrategy::UsePrebuilt {
            path,
            abi_version: args
                .runtime_abi_version
                .unwrap_or(BESKID_RUNTIME_ABI_VERSION),
        }
    } else {
        default_runtime_strategy(profile, args.target_triple.as_deref())
            .map_err(|err| anyhow::anyhow!("{err}"))?
    };

    let link_mode = match (args.prefer_static, args.prefer_dynamic) {
        (true, false) => LinkMode::PreferStatic,
        (false, true) => LinkMode::PreferDynamic,
        (true, true) => {
            return Err(anyhow::anyhow!(
                "`--prefer-static` and `--prefer-dynamic` are mutually exclusive"
            ));
        }
        (false, false) => LinkMode::Auto,
    };

    let export_policy = if args.export_symbols.is_empty() {
        ExportPolicy::PublicOnly
    } else {
        ExportPolicy::Explicit(args.export_symbols)
    };

    let link_inputs = link_libraries_for_artifact(&artifact, resolved.compile_plan.as_ref());
    let pipeline_arc: Arc<dyn PipelineObserver> = pipeline_ui.clone();
    let mut build_request = AotBuildRequest {
        artifact,
        output_kind,
        output_path: output.clone(),
        object_path: args.object_output,
        target_triple: args.target_triple,
        profile,
        entrypoint,
        export_policy,
        link_mode,
        runtime,
        verbose_link: args.verbose_link,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: Some(pipeline_arc),
    };
    apply_link_libraries(&mut build_request, link_inputs);
    let result = build(build_request)?;
    pipeline_ui.finish_build("Build complete");

    if args.plain
        && let Some(plan) = resolved.compile_plan.as_ref()
    {
        println!(
            "deps: {} materialized dependency project(s)",
            plan.dependency_projects.len()
        );
        println!(
            "corelib: {}",
            if plan.has_std_dependency {
                "available (implicit or declared)"
            } else {
                "not available"
            }
        );
    }

    println!();
    println!("  object   {}", result.object_path.display());
    if let Some(final_path) = result.final_path {
        println!("  output   {}", final_path.display());
    }
    if args.verbose_link
        && let Some(cmd) = result.linker_invocation
    {
        println!("  link     {cmd}");
    }

    Ok(())
}

fn resolve_output_kind(
    kind: Option<BuildKind>,
    target_kind: Option<TargetKind>,
) -> BuildOutputKind {
    match kind {
        Some(kind) => map_build_kind(kind),
        None => default_output_kind(target_kind.map(map_target_kind)),
    }
}

fn map_target_kind(target_kind: TargetKind) -> ProjectTargetKind {
    match target_kind {
        TargetKind::App => ProjectTargetKind::App,
        TargetKind::Lib => ProjectTargetKind::Lib,
        TargetKind::Test => ProjectTargetKind::Test,
    }
}

fn map_build_kind(kind: BuildKind) -> BuildOutputKind {
    match kind {
        BuildKind::Exe => BuildOutputKind::Exe,
        BuildKind::Shared => BuildOutputKind::SharedLib,
        BuildKind::Static => BuildOutputKind::StaticLib,
        BuildKind::Object => BuildOutputKind::ObjectOnly,
    }
}
