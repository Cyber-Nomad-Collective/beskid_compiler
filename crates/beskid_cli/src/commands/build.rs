//! `beskid build` — AOT compile and link Beskid projects to objects, libraries, or executables.

use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::syntax_codegen::{lower_prepared_entrypoint, lower_prepared_module};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use anyhow::Result;
use beskid_analysis::projects::{TargetKind, load_manifest_from_path};
use beskid_analysis::services::ResolvedInput;
use beskid_aot::{
    AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, ProjectTargetKind, build,
    default_output_kind, default_runtime_strategy, resolve_entrypoint,
};
use beskid_engine::link_libraries::{apply_link_libraries, link_libraries_for_artifact};
use beskid_pipeline::PipelineObserver;
use beskid_tools::PipelineProgressKind;
use beskid_tools::pipeline::tui::CommandSummary;
use beskid_tools::session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
use beskid_codegen::backend::BackendKind;
use clap::{Args, ValueEnum};

mod glue_rust;

/// CLI-selected output artifact shape for [`BuildArgs::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BuildKind {
    Exe,
    Shared,
    Static,
    Object,
}

/// Compile a project or source file.
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

    /// Codegen backend: `clif` (default) or `glue-rust`. `glue-rust` builds a Lib target as a shared
    /// Glue consumer plus one Rust owner image for each referenced manifest `glue` block.
    /// `glue-dotnet` is unavailable in 0.6.
    #[arg(long)]
    pub backend: Option<String>,

    /// Rust toolchain prefix for `--backend glue-rust`. Selects `<prefix>/bin/cargo` and
    /// `<prefix>/bin/rustc`. Mutually exclusive with `--cargo` and `--rustc`.
    #[arg(long, value_name = "PREFIX")]
    pub rust_toolchain: Option<PathBuf>,

    /// Cargo executable for `--backend glue-rust`. Requires `--rustc`.
    #[arg(long, value_name = "PATH")]
    pub cargo: Option<PathBuf>,

    /// rustc executable for `--backend glue-rust`. Requires `--cargo`.
    #[arg(long, value_name = "PATH")]
    pub rustc: Option<PathBuf>,

    /// Linker for the Rust owner images of `--backend glue-rust`. Required for that backend.
    #[arg(long, value_name = "PATH")]
    pub linker: Option<PathBuf>,
}

/// Backend selected for one `beskid build` invocation, after flag validation.
enum SelectedBackend {
    Clif,
    GlueRust(glue_rust::RustGlueTools),
}

/// Resolve, lower, emit CLIF, and run the AOT/link pipeline according to `args`.
pub fn execute(args: BuildArgs) -> Result<()> {
    match select_backend(&args)? {
        SelectedBackend::Clif => run_build(args),
        SelectedBackend::GlueRust(tools) => glue_rust::run(args, tools),
    }
}

/// Validate `--backend` and the backend-specific flags before any resolution or external tool.
fn select_backend(args: &BuildArgs) -> Result<SelectedBackend> {
    let kind = match args.backend.as_deref() {
        None => BackendKind::CraneliftClif,
        Some(raw) => BackendKind::parse(raw).map_err(|err| anyhow::anyhow!("{err}"))?,
    };
    match kind {
        BackendKind::CraneliftClif => {
            if args.rust_toolchain.is_some() || args.cargo.is_some() || args.rustc.is_some() || args.linker.is_some()
            {
                return Err(anyhow::anyhow!(
                    "`--rust-toolchain`, `--cargo`, `--rustc` and `--linker` apply only to `--backend glue-rust`"
                ));
            }
            Ok(SelectedBackend::Clif)
        }
        BackendKind::DotNetProject => Err(anyhow::anyhow!(
            "backend `glue-dotnet` is unavailable in 0.6; use the default `clif` backend or `--backend glue-rust`"
        )),
        BackendKind::RustSource => {
            glue_rust::reject_inapplicable_flags(args)?;
            Ok(SelectedBackend::GlueRust(glue_rust::select_tools(
                args.rust_toolchain.as_deref(),
                args.cargo.as_deref(),
                args.rustc.as_deref(),
                args.linker.as_deref(),
            )?))
        }
    }
}

fn resolve_input_args(args: &BuildArgs) -> ResolveInputArgs<'_> {
    ResolveInputArgs {
        input: args.input.as_ref(),
        project: args.project.project.as_ref(),
        target: args.project.target.as_deref(),
        workspace_member: args.project.workspace_member.as_deref(),
        frozen: args.lockfile.frozen,
        locked: args.lockfile.locked,
        offline: args.lockfile.offline,
    }
}

/// Default artifact path: `<target name or input stem>` with the platform name for `kind`, next to
/// the resolved source.
fn default_output_path(
    resolved: &ResolvedInput,
    output_kind: BuildOutputKind,
    target: &beskid_aot::target::TargetInfo,
) -> PathBuf {
    let input_path = &resolved.source_path;
    let stem = resolved.compile_plan.as_ref().map(|plan| plan.target.name.as_str()).unwrap_or_else(|| {
        input_path.file_stem().and_then(|part| part.to_str()).unwrap_or("aot_out")
    });
    let file_name = beskid_aot::target::output_filename(stem, output_kind, target);
    let parent = input_path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    parent.join(file_name)
}

fn run_build(args: BuildArgs) -> Result<()> {
    let resolve_args = resolve_input_args(&args);
    let (mut session, resolved) =
        CommandSession::open_and_resolve(args.plain, PipelineProgressKind::FullBuild, &resolve_args)?;
    if let Some(plan) = resolved.compile_plan.as_ref() {
        let manifest = load_manifest_from_path(&plan.manifest_path)?;
        if !manifest.glue.is_empty() {
            let labels = manifest.glue.iter().map(|owner| format!("`{}`", owner.library)).collect::<Vec<_>>();
            return Err(anyhow::anyhow!(
                "project `{}` declares glue block(s) {}; build it with `--backend glue-rust`, not the default `clif` backend",
                plan.project_name,
                labels.join(", ")
            ));
        }
    }
    session.set_mod_invoker(super::compiler_mod::prepare_native_mod_executor(
        &resolved,
        args.lockfile.WorkspaceOptions(),
        Some(session.observer()),
    )?);
    let prepared = session.executable_gate_prepared(
        &resolved,
        SemanticGateOptions { finish_prepare_ui: false, prepare_message: "Analysis complete" },
    )?;
    let front = prepared.into_executable()?;

    let project_target_kind = resolved.compile_plan.as_ref().map(|plan| plan.target.kind);

    let output_kind = resolve_output_kind(args.kind, project_target_kind);
    let entrypoint = resolve_entrypoint(args.entrypoint.clone())?;
    let artifact = if output_kind == BuildOutputKind::Exe {
        lower_prepared_entrypoint(&front, &entrypoint, args.target_triple.as_deref(), Some(session.observer()))?
    } else {
        lower_prepared_module(&front, args.target_triple.as_deref(), Some(session.observer()))?
    };

    let target = beskid_aot::target::detect_target(args.target_triple.as_deref())?;
    let output = match args.output {
        Some(path) => path,
        None => default_output_path(&resolved, output_kind, &target),
    };

    let profile = if args.release { BuildProfile::Release } else { BuildProfile::Debug };

    let runtime = (output_kind != BuildOutputKind::ObjectOnly)
        .then(|| default_runtime_strategy(profile, args.target_triple.as_deref()))
        .transpose()
        .map_err(|err| anyhow::anyhow!("{err}"))?;

    let link_mode = match (args.prefer_static, args.prefer_dynamic) {
        (true, false) => LinkMode::PreferStatic,
        (false, true) => LinkMode::PreferDynamic,
        (true, true) => {
            return Err(anyhow::anyhow!("`--prefer-static` and `--prefer-dynamic` are mutually exclusive"));
        }
        (false, false) => LinkMode::Auto,
    };

    let export_policy = ExportPolicy::Explicit(args.export_symbols);

    let link_inputs = link_libraries_for_artifact(&artifact, resolved.compile_plan.as_ref());
    let pipeline_arc: Arc<dyn PipelineObserver> = session.pipeline_arc();
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
    session.pipeline().finish_build_with_summary(
        "Build complete",
        CommandSummary::plain("Build", "Build complete").with_stat("output", output.display().to_string()),
    );

    if args.plain
        && let Some(plan) = resolved.compile_plan.as_ref()
    {
        println!("deps: {} materialized dependency project(s)", plan.dependency_projects.len());
        println!(
            "corelib: {}",
            if plan.has_std_dependency { "available (implicit or declared)" } else { "not available" }
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

fn resolve_output_kind(kind: Option<BuildKind>, target_kind: Option<TargetKind>) -> BuildOutputKind {
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

#[cfg(test)]
mod tests {
    use super::{BuildArgs, BuildKind, SelectedBackend, select_backend};
    use crate::cli::{Cli, Commands};
    use clap::Parser;
    use std::path::PathBuf;

    fn parse(extra: &[&str]) -> BuildArgs {
        let mut argv = vec!["beskid", "build"];
        argv.extend_from_slice(extra);
        match Cli::try_parse_from(argv).expect("parse build").command {
            Commands::Build(args) => args,
            _ => panic!("expected build command"),
        }
    }

    fn rejection(extra: &[&str]) -> String {
        match select_backend(&parse(extra)) {
            Ok(_) => panic!("{extra:?} was accepted"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn glue_rust_tool_flags_parse() {
        let args = parse(&[
            "--backend",
            "glue-rust",
            "--rust-toolchain",
            "/rust",
            "--cargo",
            "/c",
            "--rustc",
            "/r",
            "--linker",
            "/cc",
            "--kind",
            "shared",
        ]);
        assert_eq!(args.backend.as_deref(), Some("glue-rust"));
        assert_eq!(args.rust_toolchain, Some(PathBuf::from("/rust")));
        assert_eq!(args.cargo, Some(PathBuf::from("/c")));
        assert_eq!(args.rustc, Some(PathBuf::from("/r")));
        assert_eq!(args.linker, Some(PathBuf::from("/cc")));
        assert_eq!(args.kind, Some(BuildKind::Shared));
    }

    #[test]
    fn glue_rust_selects_explicit_tools() {
        let args = parse(&["--backend", "glue-rust", "--cargo", "/c", "--rustc", "/r", "--linker", "/cc"]);
        let Ok(SelectedBackend::GlueRust(tools)) = select_backend(&args) else { panic!("glue-rust not selected") };
        assert_eq!(tools.cargo, PathBuf::from("/c"));
        assert_eq!(tools.rustc, PathBuf::from("/r"));
        assert_eq!(tools.linker, PathBuf::from("/cc"));
        let shared = parse(&["--backend", "glue-rust", "--rust-toolchain", "/rust", "--linker", "/cc", "--kind", "shared"]);
        assert!(matches!(select_backend(&shared), Ok(SelectedBackend::GlueRust(_))));
    }

    #[test]
    fn default_backend_is_clif() {
        assert!(matches!(select_backend(&parse(&[])), Ok(SelectedBackend::Clif)));
        assert!(matches!(select_backend(&parse(&["--backend", "clif"])), Ok(SelectedBackend::Clif)));
    }

    #[test]
    fn glue_dotnet_fails_closed() {
        let error = rejection(&["--backend", "glue-dotnet"]);
        assert!(error.contains("`glue-dotnet` is unavailable in 0.6"), "{error}");
        assert!(!error.contains("0.5"), "{error}");
    }

    #[test]
    fn unknown_backend_is_rejected() {
        assert!(select_backend(&parse(&["--backend", "glue-go"])).is_err());
    }

    #[test]
    fn rust_tool_flags_are_rejected_for_clif() {
        for flag in ["--rust-toolchain", "--cargo", "--rustc", "--linker"] {
            let error = rejection(&[flag, "/tool"]);
            assert!(error.contains("apply only to `--backend glue-rust`"), "{flag}: {error}");
        }
    }

    #[test]
    fn glue_rust_rejects_tool_flag_mistakes() {
        for (extra, expected) in [
            (vec![], "requires explicit Rust tools"),
            (vec!["--rust-toolchain", "/rust", "--cargo", "/c", "--linker", "/cc"], "conflicts"),
            (vec!["--rust-toolchain", "/rust", "--rustc", "/r", "--linker", "/cc"], "conflicts"),
            (vec!["--cargo", "/c", "--linker", "/cc"], "`--cargo` requires `--rustc`"),
            (vec!["--rustc", "/r", "--linker", "/cc"], "`--rustc` requires `--cargo`"),
            (vec!["--cargo", "/c", "--rustc", "/r"], "requires `--linker <path>`"),
        ] {
            let mut argv = vec!["--backend", "glue-rust"];
            argv.extend(extra);
            let error = rejection(&argv);
            assert!(error.contains(expected), "{argv:?}: {error}");
        }
    }

    #[test]
    fn glue_rust_rejects_non_shared_kinds_and_inapplicable_flags() {
        let tools = ["--backend", "glue-rust", "--rust-toolchain", "/rust", "--linker", "/cc"];
        for (extra, expected) in [
            (vec!["--kind", "exe"], "`--kind exe` is not supported"),
            (vec!["--kind", "static"], "`--kind static` is not supported"),
            (vec!["--kind", "object"], "`--kind object` is not supported"),
            (vec!["--entrypoint", "Main"], "`--entrypoint` does not apply"),
            (vec!["--object-output", "x.o"], "`--object-output` does not apply"),
            (vec!["--export", "sym"], "`--export` does not apply"),
            (vec!["--prefer-static"], "`--prefer-static` does not apply"),
            (vec!["--prefer-dynamic"], "`--prefer-dynamic` does not apply"),
            (vec!["--verbose-link"], "`--verbose-link` does not apply"),
        ] {
            let mut argv = tools.to_vec();
            argv.extend(extra);
            let error = rejection(&argv);
            assert!(error.contains(expected), "{argv:?}: {error}");
        }
    }
}
