use std::path::PathBuf;

use crate::build_ui::BuildUx;
use crate::frontend;
use anyhow::Result;
use beskid_analysis::projects::TargetKind;
use beskid_aot::{
    AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, ProjectTargetKind,
    RuntimeStrategy, build, default_output_kind, resolve_entrypoint,
};
use beskid_codegen::lower_source;
use clap::{Args, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BuildKind {
    Exe,
    Shared,
    Static,
    Object,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    /// The input Beskid file to compile
    pub input: Option<PathBuf>,

    /// Path to a project directory or Project.proj file
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Target name from Project.proj
    #[arg(long)]
    pub target: Option<String>,

    /// Workspace member name when resolving from Workspace.proj
    #[arg(long = "workspace-member")]
    pub workspace_member: Option<String>,

    /// Require lockfile to be up to date and forbid lockfile updates
    #[arg(long)]
    pub frozen: bool,

    /// Require lockfile to exist and match resolution
    #[arg(long)]
    pub locked: bool,

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

    /// Runtime archive path to reuse instead of building runtime on the fly
    #[arg(long)]
    pub runtime_archive: Option<PathBuf>,

    /// ABI version for prebuilt runtime archive
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

pub fn execute(args: BuildArgs) -> Result<()> {
    let resolved = frontend::resolve_input(
        args.input.as_ref(),
        args.project.as_ref(),
        args.target.as_deref(),
        args.workspace_member.as_deref(),
        args.frozen,
        args.locked,
    )?;
    let source = resolved.source.clone();
    let input_path = resolved.source_path.clone();
    let project_target_kind = resolved.compile_plan.as_ref().map(|plan| plan.target.kind);
    let default_output_stem = resolved
        .compile_plan
        .as_ref()
        .map(|plan| plan.target.name.clone());

    let ux = BuildUx::start(!args.plain, &resolved);
    ux.stage("Parsing source and validating syntax");
    frontend::validate_source(&input_path, &source)?;

    ux.stage("Lowering source to IR");
    let artifact = lower_source(&input_path, &source, true)?.artifact;

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
            abi_version: args.runtime_abi_version,
        }
    } else {
        RuntimeStrategy::BuildOnTheFly
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

    ux.stage("Building object file, bundling runtime, linking output");
    let result = build(AotBuildRequest {
        artifact,
        output_kind,
        output_path: output.clone(),
        object_path: args.object_output,
        target_triple: args.target_triple,
        profile: if args.release {
            BuildProfile::Release
        } else {
            BuildProfile::Debug
        },
        entrypoint,
        export_policy,
        link_mode,
        runtime,
        verbose_link: args.verbose_link,
    })?;
    ux.finish("Build complete");

    println!("object: {}", result.object_path.display());
    if let Some(final_path) = result.final_path {
        println!("output: {}", final_path.display());
    }
    if let Some(cmd) = result.linker_invocation {
        println!("link: {cmd}");
    }
    if let Some(plan) = resolved.compile_plan.as_ref() {
        println!(
            "deps: {} materialized dependency project(s)",
            plan.dependency_projects.len()
        );
        println!(
            "stdlib: {}",
            if plan.has_std_dependency {
                "available (implicit or declared)"
            } else {
                "not available"
            }
        );
    }
    if ux.is_enabled() {
        println!("tip: use --plain for non-animated output");
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
