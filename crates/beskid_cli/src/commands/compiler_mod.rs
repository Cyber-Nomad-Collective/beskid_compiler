//! `beskid dev mod` - rebuild and clean compiler-mod AOT artifacts.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use beskid_analysis::projects::{
    ProjectKind, build_compile_plan, discover_project_manifest_from_input_or_cwd, discover_project_manifest_in_dir,
    discover_workspace_manifest_in_dir, load_manifest_from_path, prepare_project_workspace_with_options,
    resolve_workspace_candidate_path,
};
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan};
use beskid_aot::{ModArtifactBuildRequest, build_mod_artifact};
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{AOT_LINK, RESOLVE_GRAPH, RESOLVE_MANIFEST, WORKSPACE_GRAPH_CHANGED, WORKSPACE_MATERIALIZE},
};
use clap::{Args, Subcommand};
use walkdir::WalkDir;

use crate::project_args::LockfilePolicyArgs;
use beskid_tools::pipeline::{CliPipeline, PipelineProgressKind, tui::CommandSummary, use_cli_spinner};

#[derive(Args, Debug)]
pub struct ModArgs {
    #[command(subcommand)]
    pub command: ModCommand,
}

#[derive(Subcommand, Debug)]
pub enum ModCommand {
    /// Rebuild the AOT artifact cache entry for a compiler Mod project
    Rebuild(ModRebuildArgs),
    /// Remove cached compiler Mod AOT artifacts for a project
    Clean(ModCleanArgs),
}

#[derive(Args, Debug)]
pub struct ModRebuildArgs {
    /// Path to a Mod project directory, `.bproj`, or `.bws` manifest
    pub project: Option<PathBuf>,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Remove existing cached Mod artifacts before rebuilding
    #[arg(long)]
    pub clean: bool,

    /// Target triple override for the cached artifact
    #[arg(long)]
    pub target_triple: Option<String>,

    /// Disable animated progress and graph output
    #[arg(long)]
    pub plain: bool,
}

#[derive(Args, Debug)]
pub struct ModCleanArgs {
    /// Path to a Mod project directory, `.bproj`, or `.bws` manifest
    pub project: Option<PathBuf>,

    /// Disable animated progress and graph output
    #[arg(long)]
    pub plain: bool,
}

pub fn execute(args: ModArgs) -> Result<()> {
    match args.command {
        ModCommand::Rebuild(args) => rebuild(args),
        ModCommand::Clean(args) => clean(args),
    }
}

fn rebuild(args: ModRebuildArgs) -> Result<()> {
    let pipeline_ui = mod_pipeline(args.plain);
    let pipeline: Option<&dyn PipelineObserver> = Some(pipeline_ui.as_ref());
    let resolved = resolve_mod_project(args.project.as_ref(), pipeline)?;

    observe_phase(pipeline, WORKSPACE_GRAPH_CHANGED, || {});
    let prepared = observe_phase_result(pipeline, WORKSPACE_MATERIALIZE, || {
        prepare_project_workspace_with_options(&resolved.plan, args.lockfile.WorkspaceOptions(), pipeline)
            .map_err(anyhow::Error::from)
    })?;

    let artifact_policy = resolved
        .manifest
        .project
        .mod_section
        .as_ref()
        .map(|section| section.resolved_artifact_policy())
        .unwrap_or("rebuild");

    if args.clean || artifact_policy == "clean_rebuild" {
        remove_mod_cache_dir(&resolved.plan.project_root, &resolved.manifest.project.name)?;
    }

    let descriptor = build_mod_artifact_for_resolved(
        &resolved,
        &prepared,
        args.target_triple,
        args.lockfile.WorkspaceOptions(),
        pipeline,
    )?;

    pipeline_ui.finish_session_with_summary(
        "Mod rebuild complete",
        Some(
            CommandSummary::plain("Mod rebuild", "Mod rebuild complete")
                .with_stat("artifact", descriptor.descriptor().artifact_dir.display().to_string()),
        ),
    );
    println!("mod artifact: {}", descriptor.descriptor().artifact_dir.display());
    println!("  executable {}", descriptor.descriptor().executable_path().display());
    println!("  descriptor {}", descriptor.descriptor().sidecar_path().display());
    Ok(())
}

fn clean(args: ModCleanArgs) -> Result<()> {
    let pipeline_ui = mod_pipeline(args.plain);
    let pipeline: Option<&dyn PipelineObserver> = Some(pipeline_ui.as_ref());
    let _resolved = resolve_mod_project(args.project.as_ref(), pipeline)?;

    let removed = remove_mod_cache_dir(&_resolved.plan.project_root, &_resolved.manifest.project.name)?;
    pipeline_ui.finish_session_with_summary(
        "Mod clean complete",
        Some(CommandSummary::plain("Mod clean", "Mod clean complete")),
    );
    if removed {
        println!("removed mod artifact cache for {}", _resolved.manifest.project.name);
    } else {
        println!("no mod artifact cache found for {}", _resolved.manifest.project.name);
    }
    Ok(())
}

struct ResolvedModProject {
    manifest: beskid_analysis::projects::ProjectManifest,
    plan: beskid_analysis::projects::CompilePlan,
}

fn mod_pipeline(plain: bool) -> Arc<CliPipeline> {
    Arc::new(CliPipeline::new_with_kind(use_cli_spinner(plain), PipelineProgressKind::ModBuild))
}

fn build_mod_artifact_for_resolved(
    resolved: &ResolvedModProject,
    prepared: &beskid_analysis::projects::PreparedProjectWorkspace,
    target_triple: Option<String>,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_aot::QualifiedNativeMod> {
    let source_path = discover_mod_entry_source(&resolved.plan.source_root)?;
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read mod source {}", source_path.display()))?;
    let resolved_input = resolved_input_from_plan(
        source_path.clone(),
        source.clone(),
        resolved.plan.clone(),
        Some(prepared.clone()),
        None,
    );
    let mod_invoker = prepare_native_mod_executor(&resolved_input, policy, pipeline)?;
    let front = beskid_queries::prepare_compilation(
        &resolved_input,
        beskid_analysis::services::PrepareOptions {
            mod_invoker,
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        pipeline,
    )?
    .into_executable()?;
    let target = beskid_aot::target::detect_target(target_triple.as_deref())?;
    let abi_target = beskid_abi::abi_v5::TargetMetadata::for_triple(&target.triple)
        .map_err(|_| anyhow!("unsupported native Mod ABI target"))?;
    let prepared_mod = observe_phase_result(pipeline, beskid_pipeline::phases::CODEGEN_CLIF, || {
        beskid_aot::lower_prepared_native_mod(&front, abi_target.clone()).map_err(anyhow::Error::from)
    })?;
    let prefix = beskid_abi::runtime_kit::installed_runtime_prefix()?;
    let profile = std::env::var("BESKID_RUNTIME_KIT_PROFILE")
        .ok()
        .map(|value| beskid_abi::runtime_kit::BuildProfile::parse(&value))
        .transpose()?
        .unwrap_or(beskid_abi::runtime_kit::BuildProfile::Debug);
    let control = beskid_aot::api::NativeExecutionControl::new(
        std::time::Instant::now() + std::time::Duration::from_secs(300),
        Arc::new(|| false),
    );
    observe_phase_result(pipeline, AOT_LINK, || {
        build_mod_artifact(ModArtifactBuildRequest {
            prepared: prepared_mod,
            runtime: beskid_aot::api::RuntimeKitRequest { prefix, target: abi_target, profile },
            control,
            workspace_root: resolved.plan.project_root.clone(),
            project_root: resolved.plan.project_root.clone(),
            manifest_path: resolved.plan.manifest_path.clone(),
            source_root: resolved.plan.source_root.clone(),
            lockfile_path: Some(prepared.lockfile_path.clone()),
            package_id: resolved.manifest.project.name.clone(),
            package_version: Some(resolved.manifest.project.version.clone()),
            compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
        })
        .map_err(anyhow::Error::from)
    })
}

fn resolve_mod_project(
    project: Option<&PathBuf>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ResolvedModProject> {
    let manifest_path = observe_phase_result(pipeline, RESOLVE_MANIFEST, || resolve_manifest_path(project))?;
    let manifest = load_manifest_from_path(&manifest_path).map_err(anyhow::Error::from)?;
    if manifest.project.kind != ProjectKind::Mod {
        return Err(anyhow!("`beskid dev mod rebuild` requires a `type = Mod` project, got `{}`", manifest.project.name));
    }

    let plan = observe_phase_result(pipeline, RESOLVE_GRAPH, || {
        build_compile_plan(&manifest_path, None).map_err(anyhow::Error::from)
    })?;
    ensure_resolved_dependencies(&plan)?;

    Ok(ResolvedModProject { manifest, plan })
}

fn ensure_resolved_dependencies(plan: &beskid_analysis::projects::CompilePlan) -> Result<()> {
    if plan.unresolved_dependencies.is_empty() {
        return Ok(());
    }
    let unresolved = plan
        .unresolved_dependencies
        .iter()
        .filter(|dependency| dependency.source != beskid_analysis::projects::DependencySource::Registry)
        .map(|dependency| dependency.dependency_name.as_str())
        .collect::<Vec<_>>();
    if unresolved.is_empty() {
        return Ok(());
    }
    Err(anyhow!("unresolved mod project dependencies: {}", unresolved.join(", ")))
}

fn resolve_manifest_path(project: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(project) = project {
        return resolve_explicit_project_path(project);
    }

    discover_project_manifest_from_input_or_cwd(None, None)?
        .map(|(manifest, _summary)| manifest)
        .ok_or_else(|| anyhow!("could not discover a `.bproj` manifest from current directory"))
}

fn resolve_explicit_project_path(project: &Path) -> Result<PathBuf> {
    let candidate = if project.is_dir() {
        if let Some(manifest) = discover_project_manifest_in_dir(project).map_err(anyhow::Error::from)? {
            manifest
        } else if let Some(workspace) = discover_workspace_manifest_in_dir(project).map_err(anyhow::Error::from)? {
            workspace
        } else {
            return Err(anyhow!("no `.bproj` or `.bws` manifest found in {}", project.display()));
        }
    } else {
        project.to_path_buf()
    };

    if !candidate.is_file() {
        return Err(anyhow!("project manifest not found at {}", candidate.display()));
    }

    resolve_workspace_candidate_path(&candidate, None, None)
}

fn discover_mod_entry_source(source_root: &Path) -> Result<PathBuf> {
    for candidate in [
        source_root.join("Mod.bd"),
        source_root.join("mod.bd"),
        source_root.join("Main.bd"),
        source_root.join("main.bd"),
        source_root.join("lib.bd"),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let mut sources = WalkDir::new(source_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bd"))
        .collect::<Vec<_>>();
    sources.sort();

    if sources.len() == 1 {
        return Ok(sources.remove(0));
    }

    Err(anyhow!(
        "could not infer mod entry source under {} (expected Mod.bd, mod.bd, Main.bd, main.bd, lib.bd, or exactly one .bd file)",
        source_root.display()
    ))
}

fn mod_artifact_descriptor_exists(project_root: &Path, package_id: &str) -> bool {
    let cache_dir = project_root.join(".beskid").join("obj").join("mods").join(package_id);
    if !cache_dir.is_dir() {
        return false;
    }
    WalkDir::new(&cache_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.file_name() == "mod.descriptor.json")
}

fn remove_mod_cache_dir(project_root: &Path, package_id: &str) -> Result<bool> {
    let cache_dir = project_root.join(".beskid").join("obj").join("mods").join(package_id);
    if !cache_dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&cache_dir)
        .with_context(|| format!("failed to remove mod artifact cache {}", cache_dir.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_single_source_file_when_named_entry_is_absent() {
        let root = unique_temp_dir("beskid_cli_mod_source");
        let source_root = root.join("Src");
        fs::create_dir_all(&source_root).expect("source root");
        let only_source = source_root.join("Generator.bd");
        fs::write(&only_source, "unit Main() { return; }\n").expect("source");

        assert_eq!(discover_mod_entry_source(&source_root).expect("entry source"), only_source);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rebuilds_sample_mod_through_prepared_syntax_and_writes_registrations() {
        let root = unique_temp_dir("beskid_cli_mod_prepared_syntax");
        let source_root = root.join("Src");
        fs::create_dir_all(&source_root).expect("source root");
        let manifest_path = root.join("DemoMod.bproj");
        fs::write(
            &manifest_path,
            r#"
DemoMod {
  name = "DemoMod"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot]
  }
}
"#,
        )
        .expect("manifest");
        // Production native Mods qualify only against the actual compiler-SDK contracts; the
        // self-declared SDK-shaped contracts of `sample_mod` carry no exact canonical witness.
        fs::write(
            source_root.join("Mod.bd"),
            include_str!("../../../beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd"),
        )
        .expect("mod source");

        let manifest = load_manifest_from_path(&manifest_path).expect("load manifest");
        let plan = build_compile_plan(&manifest_path, None).expect("compile plan");
        let prepared = prepare_project_workspace_with_options(
            &plan,
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
            None,
        )
        .expect("prepare workspace");
        let descriptor = build_mod_artifact_for_resolved(
            &ResolvedModProject { manifest, plan },
            &prepared,
            Some(host_abi_target().to_owned()),
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
            None,
        )
        .expect("syntax-only mod rebuild");

        let sidecar = fs::read_to_string(descriptor.descriptor().sidecar_path()).expect("descriptor");
        let descriptor: serde_json::Value = serde_json::from_str(&sidecar).expect("descriptor json");
        let registrations = descriptor["registrations"].as_array().expect("native registrations");
        assert_eq!(registrations.len(), 2, "actual Collector and Generator must both be registered");
        for contract in ["Beskid.Compiler.Collect.Collector", "Beskid.Compiler.Collect.Generator"] {
            let registration = registrations
                .iter()
                .find(|registration| registration["contractId"] == contract)
                .unwrap_or_else(|| panic!("missing registration for {contract}"));
            assert!(
                registration["entrySymbol"].as_str().is_some_and(|symbol| !symbol.is_empty()),
                "{contract} must name its exported entry symbol"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    fn host_abi_target() -> &'static str {
        if cfg!(target_os = "macos") {
            "aarch64-apple-darwin"
        } else if cfg!(target_os = "linux") {
            "x86_64-unknown-linux-gnu"
        } else {
            "x86_64-pc-windows-msvc"
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("time").as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{id}"))
    }
}

/// Construct native authority from actual current Mod projects, never descriptor-only cache lookup.
pub(crate) fn prepare_native_mod_executor(
    resolved: &beskid_analysis::services::ResolvedInput,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Option<Arc<dyn beskid_analysis::mod_host::ContractInvoker>>> {
    let Some(plan) = resolved.compile_plan.as_ref() else { return Ok(None) };
    let mut artifacts = Vec::new();
    let mut manifests = std::collections::BTreeSet::new();
    for dependency in &plan.dependency_projects {
        let manifest_path = dependency.manifest_path.canonicalize()?;
        if !manifests.insert(manifest_path.clone()) {
            continue;
        }
        let manifest = load_manifest_from_path(&manifest_path).map_err(anyhow::Error::from)?;
        if manifest.project.kind != ProjectKind::Mod {
            continue;
        }
        let mod_plan = build_compile_plan(&manifest_path, None).map_err(anyhow::Error::from)?;
        ensure_resolved_dependencies(&mod_plan)?;
        let workspace =
            prepare_project_workspace_with_options(&mod_plan, policy.clone(), pipeline).map_err(anyhow::Error::from)?;
        artifacts.push(build_mod_artifact_for_resolved(
            &ResolvedModProject { manifest, plan: mod_plan },
            &workspace,
            None,
            policy.clone(),
            pipeline,
        )?);
    }
    if artifacts.is_empty() {
        return Ok(None);
    }
    let control = beskid_aot::api::NativeExecutionControl::new(
        std::time::Instant::now() + std::time::Duration::from_secs(300),
        Arc::new(|| false),
    );
    Ok(Some(Arc::new(beskid_aot::QualifiedModInvoker::new(artifacts, std::env::current_exe()?, control)?)))
}
