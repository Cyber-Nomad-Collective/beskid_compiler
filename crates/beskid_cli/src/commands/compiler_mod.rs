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
    // The producer identity is captured before any work, so a CLI replaced mid-build fails clearly.
    let producer = beskid_aot::ModProducerIdentity::capture().map_err(anyhow::Error::from)?;
    let runtime = beskid_analysis::mod_host::RuntimeKitSelection::installed()?;

    // Dependency Mods are inputs of this Mod: make them executable before its workspace is
    // prepared, so building one never changes a source tree the prepared proof already covers.
    let dependency_mods =
        native_mod_executor_for_plan(&resolved.plan, args.lockfile.WorkspaceOptions(), &runtime, pipeline)?;
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

    let cache = cached_mod_request(&resolved, args.target_triple.as_deref(), &runtime, &producer)?;
    let _cache_lock =
        beskid_aot::ModCacheLock::acquire(&cache.workspace_root, &cache.package_id).map_err(anyhow::Error::from)?;
    if args.clean || artifact_policy == "clean_rebuild" {
        remove_mod_cache_dir(&resolved.plan.project_root, &resolved.manifest.project.name)?;
    }

    let descriptor = build_mod_artifact_for_resolved(
        &resolved,
        &prepared,
        args.target_triple,
        &runtime,
        producer,
        dependency_mods,
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

/// Supplies the executable authority for the Mod dependencies of a Mod project being built,
/// given that project's compile plan and the lockfile policy its workspace was prepared with.
///
/// The CLI is its own Mod host ([`native_mod_executor_for_plan`]): it reuses or builds every
/// dependency Mod artifact and serves their workers. A process that embeds the producer without
/// being the CLI (a test harness) supplies an invoker qualified against the CLI instead, because
/// only the CLI serves `beskid dev native-mod-worker`.
type DependencyModExecutor<'a> = dyn Fn(
        &beskid_analysis::projects::CompilePlan,
        beskid_analysis::projects::WorkspacePrepareOptions,
    ) -> Result<beskid_analysis::mod_host::ModHost>
    + 'a;

fn mod_pipeline(plain: bool) -> Arc<CliPipeline> {
    Arc::new(CliPipeline::new_with_kind(use_cli_spinner(plain), PipelineProgressKind::ModBuild))
}

/// The producer key tuple of a resolved Mod project for the active host target and `runtime`, with
/// `producer` as the compiler identity.
fn cached_mod_request(
    resolved: &ResolvedModProject,
    target_triple: Option<&str>,
    runtime: &beskid_analysis::mod_host::RuntimeKitSelection,
    producer: &beskid_aot::ModProducerIdentity,
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
        compiler_executable: producer.executable().to_path_buf(),
        runtime: beskid_aot::api::RuntimeKitRequest {
            prefix: runtime.prefix().to_path_buf(),
            target: abi_target,
            profile: runtime.profile(),
        },
    })
}

fn build_mod_artifact_for_resolved(
    resolved: &ResolvedModProject,
    prepared: &beskid_analysis::projects::PreparedProjectWorkspace,
    target_triple: Option<String>,
    runtime: &beskid_analysis::mod_host::RuntimeKitSelection,
    producer: beskid_aot::ModProducerIdentity,
    dependency_mods: beskid_analysis::mod_host::ModHost,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_aot::QualifiedNativeMod> {
    let cache = cached_mod_request(resolved, target_triple.as_deref(), runtime, &producer)?;
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
    let front = beskid_queries::prepare_compilation(
        &resolved_input,
        beskid_analysis::services::PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            native_mod_adapter_sources: true,
            ..beskid_analysis::services::PrepareOptions::with_mod_host(dependency_mods)
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
            producer,
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
    // The package cache lock file stays: removing it while a builder holds it would let a
    // concurrent builder take a fresh lock on a new file and race the clean rebuild.
    let mut removed = false;
    for entry in fs::read_dir(&cache_dir)
        .with_context(|| format!("failed to read mod artifact cache {}", cache_dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read mod artifact cache {}", cache_dir.display()))?;
        if entry.file_name() == ".cache-lock" {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().with_context(|| format!("failed to stat {}", path.display()))?;
        if kind.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) }
            .with_context(|| format!("failed to remove mod artifact cache entry {}", path.display()))?;
        removed = true;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_tests_mod_host::ModHostScope;

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

    /// The Corelib Mods a Mod project depends on run in the CLI, the only executable that serves
    /// `beskid dev native-mod-worker`; this test harness only embeds the producer. They are
    /// provisioned with, and qualified against, the CLI for the scope's runtime kit.
    fn cli_hosted_dependency_mods(
        scope: &ModHostScope,
    ) -> impl Fn(
        &beskid_analysis::projects::CompilePlan,
        beskid_analysis::projects::WorkspacePrepareOptions,
    ) -> Result<beskid_analysis::mod_host::ModHost>
    + '_ {
        move |plan, _policy| scope.corelib_mod_host(plan)
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
    fn current_demo_mod(scope: &ModHostScope, manifest_path: &Path) -> (beskid_aot::QualifiedNativeMod, PhaseRecorder) {
        let recorder = PhaseRecorder::default();
        let artifact = current_or_built_mod_artifact(
            &resolved_demo_mod(manifest_path),
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
            scope.runtime(),
            &cli_hosted_dependency_mods(scope),
            Some(&recorder),
        )
        .expect("mod artifact");
        (artifact, recorder)
    }

    /// One sample-Mod producer run plus one deliberate stale rebuild cover the producer's
    /// registrations, its handling of unrelated generic conformances, and the artifact cache.
    ///
    /// Native Mod discovery visits every type in the assembly. A generic type conforming to an
    /// unrelated generic contract (the shape of foundation's `ArrayIterator<T> : Iterator<T>`)
    /// must be skipped by contract declaration identity, never evaluated as an applied
    /// conformance without its generic environment.
    #[test]
    fn sample_mod_registers_canonical_contracts_and_its_artifact_is_reused_until_stale() {
        let scope = ModHostScope::shared().expect("native Mod host scope");
        let root = unique_temp_dir("beskid_cli_mod_cache");
        let manifest_path = write_demo_mod(&root);
        let source = root.join("Src/Mod.bd");
        let mut text = fs::read_to_string(&source).expect("mod source");
        text.push_str(
            "\npub contract Sequence<T> { T Head(); }\n\
             pub type Holder<T> : Sequence<T> {\n    T value,\n\n    pub T Head() {\n        return this.value;\n    }\n}\n",
        );
        fs::write(&source, text).expect("mod source with an unrelated generic conformance");

        let (first, first_phases) = current_demo_mod(&scope, &manifest_path);
        assert!(first_phases.linked(), "an empty cache must run the producer");
        let sidecar = fs::read_to_string(first.descriptor().sidecar_path()).expect("descriptor");
        let descriptor: serde_json::Value = serde_json::from_str(&sidecar).expect("descriptor json");
        let registrations = descriptor["registrations"].as_array().expect("native registrations");
        assert_eq!(registrations.len(), 2, "only the canonical Collector and Generator are registered");
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
        let first_dir = first.descriptor().artifact_dir.clone();
        let cache_lock = root.join(".beskid/obj/mods/DemoMod/.cache-lock");
        fs::remove_file(&cache_lock).expect("the build held the package lock");

        let (reused, reused_phases) = current_demo_mod(&scope, &manifest_path);
        assert!(!reused_phases.linked(), "a current cache entry must be reused without codegen or link");
        assert!(
            !cache_lock.exists(),
            "qualifying a current artifact is read-only: no lock file is created (installed prefixes stay intact)"
        );
        assert_eq!(reused.descriptor().artifact_dir, first_dir);
        reused.verify_native_closure().expect("reused artifact keeps its verified native closure");

        let mut text = fs::read_to_string(&source).expect("mod source");
        // A structural edit: one process's syntax database admits a changed source only with a
        // new expanded tree, so a trivia-only edit is not a new Mod revision there.
        text.push_str("\npub unit StaleSourceMarker() { return; }\n");
        fs::write(&source, text).expect("edit mod source");
        let (rebuilt, rebuilt_phases) = current_demo_mod(&scope, &manifest_path);
        assert!(rebuilt_phases.linked(), "a changed source must rebuild the Mod");
        assert_ne!(rebuilt.descriptor().artifact_dir, first_dir, "stale evidence never selects the old entry");
        assert!(!first_dir.exists(), "the superseded entry of the same target and profile is pruned");

        let (_, again) = current_demo_mod(&scope, &manifest_path);
        assert!(!again.linked(), "the rebuilt entry is current again");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shipped_mod_artifact_is_qualified_for_a_non_building_consumer() {
        let scope = ModHostScope::shared().expect("native Mod host scope");
        let root = unique_temp_dir("beskid_cli_mod_shipped");
        let mod_root = root.join("DemoMod");
        let manifest_path = write_demo_mod(&mod_root);
        // The provisioning step: the toolchain CLI builds the descriptor into the Mod project. It
        // is the Mod host that a non-building consumer later qualifies the artifact against.
        let cli = beskid_tests_mod_host::provisioned_cli().expect("Mod host CLI");
        let output = std::process::Command::new(&cli)
            .args(["dev", "mod", "rebuild"])
            .arg(&manifest_path)
            .args(["--offline", "--plain"])
            .env("BESKID_RUNTIME_PREFIX", scope.runtime().prefix())
            .env("BESKID_RUNTIME_KIT_PROFILE", "debug")
            .output()
            .expect("run `beskid dev mod rebuild`");
        assert!(
            output.status.success(),
            "provision the shipped Mod artifact\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let shipped = mod_root.join(".beskid/obj/mods/DemoMod");
        assert!(
            WalkDir::new(&shipped).into_iter().filter_map(Result::ok).any(|entry| entry.file_name() == "mod.descriptor.json"),
            "the CLI ships the descriptor inside the Mod project under {}",
            shipped.display()
        );

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

        // The editor path qualifies it for execution against the Mod host that produced it and the
        // runtime kit it was built for, both passed as values.
        let mod_host = beskid_tools::native_mods::cached_mod_host_for_plan(&host_plan, &cli, scope.runtime())
            .expect("non-building consumer qualifies the shipped artifact");
        assert!(mod_host.is_native(), "a Mod dependency yields a native Mod host");
        assert_eq!(mod_host.runtime(), Some(scope.runtime()));

        // Installers move the whole bundle to its install prefix: the shipped artifact carries no
        // absolute path, so it still qualifies after relocation.
        let relocated_root = root.join("installed/DemoMod");
        copy_tree(&mod_root, &relocated_root);
        let relocated_host = root.join("RelocatedHost");
        fs::create_dir_all(relocated_host.join("Src")).expect("relocated host source");
        fs::write(relocated_host.join("Src/Main.bd"), "unit Main() { return; }\n").expect("relocated host entry");
        let relocated_manifest = relocated_host.join("Host.bproj");
        fs::write(
            &relocated_manifest,
            format!(
                "Host {{ name = \"Host\" version = \"0.1.0\" root = \"Src\" }}\ndependency \"DemoMod\" {{ source = path path = {:?} }}\ntarget \"main\" {{ kind = App entry = \"Main.bd\" }}\n",
                relocated_root.to_string_lossy()
            ),
        )
        .expect("relocated host manifest");
        let relocated_plan = build_compile_plan(&relocated_manifest, None).expect("relocated host plan");
        assert!(
            beskid_tools::native_mods::cached_mod_host_for_plan(&relocated_plan, &cli, scope.runtime())
                .expect("a relocated shipped artifact qualifies without a rebuild")
                .is_native()
        );

        // A different Mod host executable never qualifies the artifact; the error names the remedy.
        let foreign = root.join("foreign-host");
        fs::write(&foreign, b"not the producing compiler").expect("foreign host");
        let error = beskid_tools::native_mods::cached_mod_host_for_plan(&host_plan, &foreign, scope.runtime())
            .err()
            .expect("foreign Mod host must fail closed")
            .to_string();
        assert!(error.contains("beskid dev mod rebuild"), "{error}");

        // Without a descriptor qualification fails with the same actionable remedy.
        fs::remove_dir_all(mod_root.join(".beskid")).expect("remove shipped artifact");
        let error = beskid_tools::native_mods::cached_mod_host_for_plan(&host_plan, &cli, scope.runtime())
            .err()
            .expect("missing descriptor fails closed")
            .to_string();
        assert!(error.contains("beskid dev mod rebuild"), "{error}");
        let _ = fs::remove_dir_all(root);
    }

    /// Mod activation follows reach: a program that never reaches the serialization package does
    /// not build or run the Serialization Mod that the Corelib aggregate pulls into its graph; a
    /// program that imports that package's surface activates it.
    #[test]
    fn corelib_mods_activate_only_for_programs_that_reach_their_package() {
        beskid_tools::ensure_bundled_corelib(&beskid_corelib_embed::CORELIB_BUNDLE).expect("provision the installed Corelib");
        let root = unique_temp_dir("beskid_cli_mod_activation");
        let host_input = |name: &str, source: &str| {
            let host = root.join(name);
            fs::create_dir_all(host.join("Src")).expect("host source");
            fs::write(host.join("Src/Main.bd"), source).expect("host entry");
            let manifest = host.join("Host.bproj");
            fs::write(
                &manifest,
                "Host { name = \"Host\" version = \"0.1.0\" root = \"Src\" }\ntarget \"main\" { kind = App entry = \"Main.bd\" }\n",
            )
            .expect("host manifest");
            let plan = build_compile_plan(&manifest, None).expect("host plan");
            resolved_input_from_plan(host.join("Src/Main.bd"), source.to_owned(), plan, None, None)
        };
        let is_serialization_mod =
            |path: &PathBuf| path.file_name().is_some_and(|name| name == "serialization_mod.bproj");

        let hello = host_input("Hello", "unit Main() { return; }\n");
        let plan = hello.compile_plan.as_ref().expect("plan");
        assert!(
            plan.dependency_projects.iter().any(|dependency| dependency.project_name == "serialization_mod"),
            "the implicit Corelib aggregate puts the Serialization Mod in every graph"
        );
        let active = beskid_analysis::mod_host::active_mod_manifests_for_input(&hello, &FrontEndOptions::default(), false)
            .expect("hello-world activation");
        assert!(active.is_empty(), "hello-world activates no Mod: {active:?}");
        let host = prepare_native_mod_executor(&hello, Default::default(), None).expect("hello-world Mod host");
        assert!(!host.is_native(), "hello-world needs no native Mod host, so nothing is built");

        let serializing = host_input(
            "Serializing",
            "use Core.Serialization.Contracts;\nunit Main() { return; }\n",
        );
        let active =
            beskid_analysis::mod_host::active_mod_manifests_for_input(&serializing, &FrontEndOptions::default(), false)
                .expect("serialization activation");
        assert!(active.iter().any(is_serialization_mod), "importing Core.Serialization activates its Mod: {active:?}");
        let _ = fs::remove_dir_all(root);
    }

    /// A Corelib Mod inside an installed toolchain prefix is never built on demand; one in a
    /// managed per-user Corelib (materialized from the embedded snapshot) is.
    #[test]
    fn only_corelib_bundles_of_an_installed_prefix_are_immutable() {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib");
        let prefix = unique_temp_dir("beskid_cli_installed_prefix");
        let bundle = prefix.join("beskid_corelib");
        beskid_abi::corelib_bundle::copy_corelib_bundle(&checkout, &bundle).expect("copy Corelib bundle");
        let fingerprint = beskid_abi::corelib_bundle::fingerprint_corelib_bundle_dir(&bundle).expect("fingerprint");
        fs::write(bundle.join(beskid_abi::corelib_bundle::CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n"))
            .expect("seal bundle");
        let serialization_mod = bundle.join("mods/serialization_mod/serialization_mod.bproj");
        assert_eq!(installed_toolchain_prefix_of(&serialization_mod), None, "a managed Corelib is a cache");
        fs::write(prefix.join("release-version.txt"), "0.6.0\n").expect("release layout");
        assert_eq!(
            installed_toolchain_prefix_of(&serialization_mod),
            Some(prefix.clone()),
            "the bundle of an installed prefix is immutable"
        );
        let user_mod = write_demo_mod(&prefix.join("user/DemoMod"));
        assert_eq!(installed_toolchain_prefix_of(&user_mod), None, "a user Mod is always built on demand");
        let _ = fs::remove_dir_all(prefix);
    }

    fn copy_tree(from: &Path, to: &Path) {
        for entry in WalkDir::new(from).into_iter().map(|entry| entry.expect("walk")) {
            let target = to.join(entry.path().strip_prefix(from).expect("relative"));
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target).expect("copy directory");
            } else {
                fs::copy(entry.path(), &target).expect("copy file");
            }
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
/// current, otherwise a fresh build.
///
/// Qualification is strictly read-only: the cache is consulted before any lock, workspace
/// preparation or write, so a hit never touches the Mod project. An installed toolchain prefix
/// stays byte-identical to its install receipt and a read-only (package-managed) prefix works.
/// Publication is an atomic no-replace rename, so a lock-free read never observes a partial entry.
/// Only a miss takes the per-package build lock and qualifies again under it, so a concurrent
/// builder's fresh publication is reused instead of rebuilt.
///
/// A Corelib Mod of an installed toolchain is never built on demand: the toolchain ships it
/// prebuilt for its own CLI and the debug runtime kit, as rustup ships a prebuilt standard
/// library, and the install prefix is immutable. Without a current shipped artifact the build
/// fails closed with the remedy. Every other Mod is built once into its own project cache, as
/// Cargo builds a procedural-macro crate on demand.
fn current_or_built_mod_artifact(
    resolved_mod: &ResolvedModProject,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    runtime: &beskid_analysis::mod_host::RuntimeKitSelection,
    dependency_mods: &DependencyModExecutor<'_>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_aot::QualifiedNativeMod> {
    // The producer identity is captured before any work, so a CLI replaced mid-build fails clearly.
    let producer = beskid_aot::ModProducerIdentity::capture().map_err(anyhow::Error::from)?;
    let cache = cached_mod_request(resolved_mod, None, runtime, &producer)?;
    // Lock-free, this read may observe a cooperating publisher's prune; an integrity failure is
    // decided only by the qualification under the package cache lock below.
    let unlocked = beskid_aot::qualify_cached_mod_artifact(&cache);
    if let Ok(Some(current)) = unlocked {
        return Ok(current);
    }
    if let Some(prefix) = installed_toolchain_prefix_of(&resolved_mod.plan.manifest_path) {
        // An installed prefix is immutable and never pruned, so its integrity failure is final.
        unlocked.map_err(anyhow::Error::from)?;
        anyhow::bail!(
            "the installed toolchain at {} ships Corelib Mod `{}` prebuilt, but has no current artifact for this \
             `beskid` ({}), target {} and the {:?} runtime kit; an installed toolchain is never modified, so the \
             Mod is not built on demand. Update the toolchain (`beskid toolchain update`) so the shipped artifact matches its \
             CLI, or use the debug runtime kit (unset BESKID_RUNTIME_KIT_PROFILE), which installed toolchains ship",
            prefix.display(),
            cache.package_id,
            cache.compiler_executable.display(),
            cache.runtime.target.triple.as_str(),
            cache.runtime.profile
        );
    }
    // One builder per package cache at a time: a concurrent compilation that depends on the
    // same Mod waits here and then qualifies the fresh publication instead of rebuilding and
    // pruning underneath this one.
    let _cache_lock =
        beskid_aot::ModCacheLock::acquire(&cache.workspace_root, &cache.package_id).map_err(anyhow::Error::from)?;
    if let Some(current) = beskid_aot::qualify_cached_mod_artifact(&cache).map_err(anyhow::Error::from)? {
        return Ok(current);
    }
    let mod_policy = bundled_mod_policy(&resolved_mod.plan.manifest_path, policy);
    // Dependency Mods first: building one must not change a tree the prepared proof covers.
    let dependency_host = dependency_mods(&resolved_mod.plan, mod_policy.clone())?;
    let workspace = prepare_project_workspace_with_options(&resolved_mod.plan, mod_policy, pipeline)
        .map_err(anyhow::Error::from)?;
    build_mod_artifact_for_resolved(resolved_mod, &workspace, None, runtime, producer, dependency_host, pipeline)
}

/// The installation prefix whose sealed Corelib bundle holds `manifest_path`, when that bundle is
/// part of an installed toolchain (`<prefix>/beskid_corelib` beside `<prefix>/release-version.txt`,
/// the release bundle layout every distribution channel installs). A Corelib materialized from the
/// CLI's embedded snapshot into a per-user root is a managed cache, not an installed prefix.
fn installed_toolchain_prefix_of(manifest_path: &Path) -> Option<PathBuf> {
    let bundle = beskid_abi::corelib_bundle::verified_corelib_bundle_root(manifest_path)?;
    let prefix = bundle.parent()?;
    prefix.join("release-version.txt").is_file().then(|| prefix.to_path_buf())
}

/// Construct the native Mod host for the Mods `resolved` activates (`mod_host::activation`), with
/// this CLI as the host and the runtime kit it resolves for itself. A Mod of a package the program
/// never reaches is neither built nor run. Each active Mod's artifact is reused only when
/// producer-side cache qualification proves its complete key tuple current; otherwise it is built.
/// A descriptor alone never grants execution authority. An input that activates no Mod yields
/// [`beskid_analysis::mod_host::ModHost::NoMods`].
pub(crate) fn prepare_native_mod_executor(
    resolved: &beskid_analysis::services::ResolvedInput,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_analysis::mod_host::ModHost> {
    // Every caller prepares with the default assembly policy (import closure for an entry,
    // workspace scan for an entry-less library), so activation is decided on that same assembly.
    let active =
        beskid_analysis::mod_host::active_mod_manifests_for_input(resolved, &FrontEndOptions::default(), false)?;
    native_mod_host_for_active(&active, policy, pipeline)
}

/// [`prepare_native_mod_executor`] for one session that prepares several inputs (the test matrix):
/// the host serves the union of the Mods the inputs activate. An input whose activation cannot be
/// decided (its graph or assembly fails) contributes nothing here; its own preparation reports
/// that same failure for that input alone.
pub(crate) fn prepare_native_mod_executor_for_inputs(
    inputs: &[beskid_analysis::services::ResolvedInput],
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_analysis::mod_host::ModHost> {
    let mut active = std::collections::BTreeSet::new();
    for input in inputs {
        if let Ok(manifests) =
            beskid_analysis::mod_host::active_mod_manifests_for_input(input, &FrontEndOptions::default(), false)
        {
            active.extend(manifests);
        }
    }
    native_mod_host_for_active(&active.into_iter().collect::<Vec<_>>(), policy, pipeline)
}

fn native_mod_host_for_active(
    active: &[PathBuf],
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_analysis::mod_host::ModHost> {
    if active.is_empty() {
        return Ok(beskid_analysis::mod_host::ModHost::NoMods);
    }
    let runtime = beskid_analysis::mod_host::RuntimeKitSelection::installed()?;
    native_mod_host_for_manifests(active, policy, &runtime, pipeline)
}

/// The Mod host for building the Mod project of `plan`: the Mods its own assembly activates, which
/// is assembled with the compiler-owned SDK adapter sources exactly as its build prepares it.
fn native_mod_executor_for_plan(
    plan: &beskid_analysis::projects::CompilePlan,
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    runtime: &beskid_analysis::mod_host::RuntimeKitSelection,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_analysis::mod_host::ModHost> {
    let source_path = discover_mod_entry_source(&plan.source_root)?;
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read mod source {}", source_path.display()))?;
    let resolved = resolved_input_from_plan(source_path, source, plan.clone(), None, None);
    let active =
        beskid_analysis::mod_host::active_mod_manifests_for_input(&resolved, &FrontEndOptions::default(), true)?;
    native_mod_host_for_manifests(&active, policy, runtime, pipeline)
}

/// Reuse or build the artifact of every Mod in `manifests` and serve them from this CLI.
fn native_mod_host_for_manifests(
    manifests: &[PathBuf],
    policy: beskid_analysis::projects::WorkspacePrepareOptions,
    runtime: &beskid_analysis::mod_host::RuntimeKitSelection,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_analysis::mod_host::ModHost> {
    let mut artifacts = Vec::new();
    for manifest_path in manifests {
        let manifest = load_manifest_from_path(manifest_path).map_err(anyhow::Error::from)?;
        if manifest.project.kind != ProjectKind::Mod {
            anyhow::bail!("active Mod dependency {} is not a `type = Mod` project", manifest_path.display());
        }
        let mod_plan = build_compile_plan(manifest_path, None).map_err(anyhow::Error::from)?;
        ensure_resolved_dependencies(&mod_plan)?;
        let resolved_mod = ResolvedModProject { manifest, plan: mod_plan };
        artifacts.push(current_or_built_mod_artifact(
            &resolved_mod,
            policy.clone(),
            runtime,
            &|plan, policy| native_mod_executor_for_plan(plan, policy, runtime, pipeline),
            pipeline,
        )?);
    }
    if artifacts.is_empty() {
        return Ok(beskid_analysis::mod_host::ModHost::NoMods);
    }
    let invoker = beskid_aot::QualifiedModInvoker::new(
        artifacts,
        std::env::current_exe()?,
        std::time::Duration::from_secs(300),
        Arc::new(|| false),
    )?;
    Ok(beskid_analysis::mod_host::ModHost::native(Arc::new(invoker), runtime.clone()))
}
