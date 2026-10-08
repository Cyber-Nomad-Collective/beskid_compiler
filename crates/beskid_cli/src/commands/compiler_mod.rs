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

/// The producer key tuple of a resolved Mod project for the active host target and runtime profile.
fn cached_mod_request(
    resolved: &ResolvedModProject,
    target_triple: Option<&str>,
) -> Result<beskid_aot::CachedModArtifactRequest> {
    let target = beskid_aot::target::detect_target(target_triple)?;
    let abi_target = beskid_abi::abi_v5::TargetMetadata::for_triple(&target.triple)
        .map_err(|_| anyhow!("unsupported native Mod ABI target"))?;
    Ok(beskid_aot::CachedModArtifactRequest {
        workspace_root: resolved.plan.project_root.clone(),
        project_root: resolved.plan.project_root.clone(),
        manifest_path: resolved.plan.manifest_path.clone(),
        source_root: resolved.plan.source_root.clone(),
        package_id: resolved.manifest.project.name.clone(),
        dependency_sources: beskid_analysis::mod_host::native_mod_dependency_sources(&resolved.plan)?,
        compiler_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        compiler_executable: std::env::current_exe().context("locate the Mod-host compiler executable")?,
        runtime: beskid_aot::api::RuntimeKitRequest {
            prefix: beskid_abi::runtime_kit::installed_runtime_prefix()?,
            target: abi_target,
            profile: beskid_analysis::mod_host::native_mod_runtime_profile()?,
        },
    })
}

fn build_mod_artifact_for_resolved(
    resolved: &ResolvedModProject,
    prepared: &beskid_analysis::projects::PreparedProjectWorkspace,
    target_triple: Option<String>,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_aot::QualifiedNativeMod> {
    let cache = cached_mod_request(resolved, target_triple.as_deref())?;
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
            native_mod_adapter_sources: true,
            ..Default::default()
        },
        pipeline,
    )?
    .into_executable()?;
    let abi_target = cache.runtime.target.clone();
    let prepared_mod = observe_phase_result(pipeline, beskid_pipeline::phases::CODEGEN_CLIF, || {
        beskid_aot::lower_prepared_native_mod(&front, abi_target.clone()).map_err(anyhow::Error::from)
    })?;
    let control = beskid_aot::api::NativeExecutionControl::new(
        std::time::Instant::now() + std::time::Duration::from_secs(300),
        Arc::new(|| false),
    );
    observe_phase_result(pipeline, AOT_LINK, || {
        build_mod_artifact(ModArtifactBuildRequest {
            prepared: prepared_mod,
            runtime: cache.runtime,
            control,
            workspace_root: cache.workspace_root,
            project_root: cache.project_root,
            manifest_path: cache.manifest_path,
            source_root: cache.source_root,
            lockfile_path: Some(prepared.lockfile_path.clone()),
            package_id: cache.package_id,
            package_version: Some(resolved.manifest.project.version.clone()),
            dependency_sources: cache.dependency_sources,
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

    /// Native Mod discovery visits every type in the assembly. A generic type conforming to an
    /// unrelated generic contract (the shape of foundation's `ArrayIterator<T> : Iterator<T>`)
    /// must be skipped by contract declaration identity, never evaluated as an applied
    /// conformance without its generic environment.
    #[test]
    fn rebuilds_mod_whose_assembly_contains_unrelated_generic_conformance() {
        let root = unique_temp_dir("beskid_cli_mod_generic_conformance");
        let manifest_path = write_demo_mod(&root);
        let mut source = include_str!("../../../beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd").to_owned();
        source.push_str(
            "\npub contract Sequence<T> { T Head(); }\n\
             pub type Holder<T> : Sequence<T> {\n    T value,\n\n    pub T Head() {\n        return this.value;\n    }\n}\n",
        );
        fs::write(root.join("Src").join("Mod.bd"), source).expect("mod source");
        let descriptor = rebuilt_demo_mod(&manifest_path);
        let sidecar = fs::read_to_string(descriptor.descriptor().sidecar_path()).expect("descriptor");
        let descriptor: serde_json::Value = serde_json::from_str(&sidecar).expect("descriptor json");
        let registrations = descriptor["registrations"].as_array().expect("native registrations");
        assert_eq!(registrations.len(), 2, "only the canonical Collector and Generator are registered");
        let _ = fs::remove_dir_all(root);
    }

    /// Records phase starts so a test can prove whether the producer (codegen + link) ran.
    #[derive(Default)]
    struct PhaseRecorder(std::sync::Mutex<Vec<&'static str>>);
    impl PipelineObserver for PhaseRecorder {
        fn on_event(&self, event: beskid_pipeline::PipelineEvent) {
            if let beskid_pipeline::PipelineEvent::PhaseStart { id } = event {
                self.0.lock().expect("phase log").push(id);
            }
        }
    }
    impl PhaseRecorder {
        fn linked(&self) -> bool {
            self.0.lock().expect("phase log").contains(&AOT_LINK)
        }
    }

    fn write_demo_mod(root: &Path) -> PathBuf {
        let source_root = root.join("Src");
        fs::create_dir_all(&source_root).expect("source root");
        let manifest_path = root.join("DemoMod.bproj");
        fs::write(
            &manifest_path,
            "DemoMod {\n  name = \"DemoMod\"\n  version = \"0.1.0\"\n  type = Mod\n  mod {\n    capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot]\n  }\n}\n",
        )
        .expect("manifest");
        fs::write(
            source_root.join("Mod.bd"),
            include_str!("../../../beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd"),
        )
        .expect("mod source");
        manifest_path
    }

    fn resolved_demo_mod(manifest_path: &Path) -> ResolvedModProject {
        let manifest = load_manifest_from_path(manifest_path).expect("load manifest");
        let plan = build_compile_plan(manifest_path, None).expect("compile plan");
        ResolvedModProject { manifest, plan }
    }

    /// The dependency path used by every consumer that builds on demand (`build`, `run`, `check`).
    fn current_demo_mod(manifest_path: &Path) -> (beskid_aot::QualifiedNativeMod, PhaseRecorder) {
        let recorder = PhaseRecorder::default();
        let artifact = current_or_built_mod_artifact(
            &resolved_demo_mod(manifest_path),
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
            Some(&recorder),
        )
        .expect("mod artifact");
        (artifact, recorder)
    }

    /// The provisioning path (`beskid dev mod rebuild`), which always runs the producer.
    fn rebuilt_demo_mod(manifest_path: &Path) -> beskid_aot::QualifiedNativeMod {
        let resolved = resolved_demo_mod(manifest_path);
        let prepared = prepare_project_workspace_with_options(
            &resolved.plan,
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
            None,
        )
        .expect("prepare workspace");
        build_mod_artifact_for_resolved(
            &resolved,
            &prepared,
            Some(host_abi_target().to_owned()),
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
            None,
        )
        .expect("mod rebuild")
    }

    #[test]
    fn current_mod_artifact_is_reused_without_rebuild_and_stale_source_rebuilds() {
        let root = unique_temp_dir("beskid_cli_mod_cache");
        let manifest_path = write_demo_mod(&root);

        let (first, first_phases) = current_demo_mod(&manifest_path);
        assert!(first_phases.linked(), "an empty cache must run the producer");
        let first_dir = first.descriptor().artifact_dir.clone();

        let (reused, reused_phases) = current_demo_mod(&manifest_path);
        assert!(!reused_phases.linked(), "a current cache entry must be reused without codegen or link");
        assert_eq!(reused.descriptor().artifact_dir, first_dir);
        reused.verify_native_closure().expect("reused artifact keeps its verified native closure");

        let source = root.join("Src/Mod.bd");
        let mut text = fs::read_to_string(&source).expect("mod source");
        text.push_str("\n// stale-source cache invalidation\n");
        fs::write(&source, text).expect("edit mod source");
        let (rebuilt, rebuilt_phases) = current_demo_mod(&manifest_path);
        assert!(rebuilt_phases.linked(), "a changed source must rebuild the Mod");
        assert_ne!(rebuilt.descriptor().artifact_dir, first_dir, "stale evidence never selects the old entry");
        assert!(!first_dir.exists(), "the superseded entry of the same target and profile is pruned");

        let (_, again) = current_demo_mod(&manifest_path);
        assert!(!again.linked(), "the rebuilt entry is current again");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shipped_mod_artifact_is_qualified_for_a_non_building_consumer() {
        let root = unique_temp_dir("beskid_cli_mod_shipped");
        let mod_root = root.join("DemoMod");
        let manifest_path = write_demo_mod(&mod_root);
        // The provisioning step: the toolchain CLI builds the descriptor into the Mod project.
        let shipped = rebuilt_demo_mod(&manifest_path);
        assert!(shipped.descriptor().artifact_dir.starts_with(mod_root.join(".beskid/obj/mods/DemoMod")));

        let host = root.join("Host");
        fs::create_dir_all(host.join("Src")).expect("host source");
        fs::write(host.join("Src/Main.bd"), "unit Main() { return; }\n").expect("host entry");
        let host_manifest = host.join("Host.bproj");
        fs::write(
            &host_manifest,
            format!(
                "Host {{ name = \"Host\" version = \"0.1.0\" root = \"Src\" }}\ndependency \"DemoMod\" {{ source = path path = {:?} }}\ntarget \"main\" {{ kind = App entry = \"Main.bd\" }}\n",
                mod_root.to_string_lossy()
            ),
        )
        .expect("host manifest");
        let host_plan = build_compile_plan(&host_manifest, None).expect("host plan");

        // The analysis loader (queries / engine path) finds the descriptor without building.
        beskid_analysis::mod_host::native_invoker_for_plan(&host_plan, None)
            .expect("loader finds the shipped descriptor")
            .expect("host has a Mod dependency");
        // The editor path qualifies it for execution against the Mod host that produced it.
        let compiler = std::env::current_exe().expect("Mod host executable");
        let invoker = beskid_tools::native_mods::cached_mod_invoker_for_plan(&host_plan, &compiler)
            .expect("non-building consumer qualifies the shipped artifact");
        assert!(invoker.is_some(), "a Mod dependency yields a qualified invoker");

        // A different Mod host executable never qualifies the artifact; the error names the remedy.
        let foreign = root.join("foreign-host");
        fs::write(&foreign, b"not the producing compiler").expect("foreign host");
        let error = beskid_tools::native_mods::cached_mod_invoker_for_plan(&host_plan, &foreign)
            .err()
            .expect("foreign Mod host must fail closed")
            .to_string();
        assert!(error.contains("beskid dev mod rebuild"), "{error}");

        // Without a descriptor the loader fails with the same actionable remedy.
        fs::remove_dir_all(mod_root.join(".beskid")).expect("remove shipped artifact");
        let error = beskid_analysis::mod_host::native_invoker_for_plan(&host_plan, None)
            .err()
            .expect("missing descriptor fails closed")
            .to_string();
        assert!(error.contains("beskid dev mod rebuild"), "{error}");
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

/// A Mod inside a verified Corelib bundle is sealed by the bundle fingerprint and ships without a
/// generated `Project.lock` (release bundles never embed lockfiles). The consumer's `--locked` /
/// `--frozen` policy governs the consumer's own graph; the sealed Mod closure is resolved offline
/// from the verified bundle, so its own lock is generated rather than required.
fn bundled_mod_policy(
    manifest_path: &Path,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
) -> beskid_analysis::projects::WorkspacePrepareOptions {
    if beskid_abi::corelib_bundle::verified_corelib_bundle_root(manifest_path).is_some() {
        beskid_analysis::projects::WorkspacePrepareOptions { locked: false, frozen: false, ..policy }
    } else {
        policy
    }
}

/// The executable artifact of a dependency Mod for the host target: a current cached or
/// toolchain-shipped artifact when producer-side qualification proves its complete key tuple
/// current, otherwise a fresh build. The cache is consulted before any workspace preparation, so
/// a hit never writes into the Mod project (an installed toolchain prefix stays byte-identical to
/// its install receipt).
fn current_or_built_mod_artifact(
    resolved_mod: &ResolvedModProject,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_aot::QualifiedNativeMod> {
    if let Some(current) =
        beskid_aot::qualify_cached_mod_artifact(&cached_mod_request(resolved_mod, None)?).map_err(anyhow::Error::from)?
    {
        return Ok(current);
    }
    let mod_policy = bundled_mod_policy(&resolved_mod.plan.manifest_path, policy);
    let workspace = prepare_project_workspace_with_options(&resolved_mod.plan, mod_policy.clone(), pipeline)
        .map_err(anyhow::Error::from)?;
    build_mod_artifact_for_resolved(resolved_mod, &workspace, None, mod_policy, pipeline)
}

/// Construct native authority from the actual current Mod projects. Each Mod's artifact is reused
/// only when producer-side cache qualification proves its complete key tuple current; otherwise
/// it is built. A descriptor alone never grants execution authority.
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
        let resolved_mod = ResolvedModProject { manifest, plan: mod_plan };
        artifacts.push(current_or_built_mod_artifact(&resolved_mod, policy.clone(), pipeline)?);
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
