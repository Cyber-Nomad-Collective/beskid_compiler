//! `beskid mod` - rebuild and clean compiler-mod AOT artifacts.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use beskid_analysis::projects::{
    PROJECT_FILE_NAME, ProjectKind, WorkspacePrepareOptions, build_compile_plan,
    discover_project_manifest_from_input_or_cwd, load_manifest_from_path,
    prepare_project_workspace_with_options, resolve_workspace_candidate_path,
};
use beskid_analysis::services::resolved_input_from_plan;
use beskid_aot::{ModArtifactBuildRequest, build_mod_artifact};
use beskid_codegen::lower_resolved_input_with_pipeline;
use beskid_pipeline::{
    PipelineObserver, observe_phase, observe_phase_result,
    phases::{
        AOT_LINK, RESOLVE_GRAPH, RESOLVE_MANIFEST, WORKSPACE_GRAPH_CHANGED, WORKSPACE_MATERIALIZE,
    },
};
use clap::{Args, Subcommand};
use walkdir::WalkDir;

use crate::project_args::LockfilePolicyArgs;
use beskid_tools::pipeline::{CliPipeline, PipelineProgressKind, use_cli_spinner};

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
    /// Path to a Mod project directory, Project.proj, or Workspace.proj
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
    /// Path to a Mod project directory, Project.proj, or Workspace.proj
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
        prepare_project_workspace_with_options(
            &resolved.plan,
            WorkspacePrepareOptions {
                frozen: args.lockfile.frozen,
                locked: args.lockfile.locked,
            },
        )
        .map_err(anyhow::Error::from)
    })?;

    if args.clean {
        remove_mod_cache_dir(&resolved.plan.project_root, &resolved.manifest.project.name)?;
    }

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
    let lowered = lower_resolved_input_with_pipeline(&resolved_input, false, pipeline)?;
    let target = beskid_aot::target::detect_target(args.target_triple.as_deref())?;
    let descriptor = observe_phase_result(pipeline, AOT_LINK, || {
        build_mod_artifact(ModArtifactBuildRequest {
            artifact: lowered.artifact,
            workspace_root: resolved.plan.project_root.clone(),
            project_root: resolved.plan.project_root.clone(),
            manifest_path: resolved.plan.manifest_path.clone(),
            source_root: resolved.plan.source_root.clone(),
            lockfile_path: Some(prepared.lockfile_path.clone()),
            package_id: resolved.manifest.project.name.clone(),
            package_version: Some(resolved.manifest.project.version.clone()),
            target_triple: target.triple.clone(),
            compiler_version: env!("CARGO_PKG_VERSION").to_owned(),
            registrations: Vec::new(),
        })
        .map_err(anyhow::Error::from)
    })?;

    pipeline_ui.finish_session("Mod rebuild complete");
    println!("mod artifact: {}", descriptor.artifact_dir.display());
    println!("  object     {}", descriptor.object_path().display());
    println!("  descriptor {}", descriptor.sidecar_path().display());
    Ok(())
}

fn clean(args: ModCleanArgs) -> Result<()> {
    let pipeline_ui = mod_pipeline(args.plain);
    let pipeline: Option<&dyn PipelineObserver> = Some(pipeline_ui.as_ref());
    let _resolved = resolve_mod_project(args.project.as_ref(), pipeline)?;

    let removed = remove_mod_cache_dir(
        &_resolved.plan.project_root,
        &_resolved.manifest.project.name,
    )?;
    pipeline_ui.finish_session("Mod clean complete");
    if removed {
        println!(
            "removed mod artifact cache for {}",
            _resolved.manifest.project.name
        );
    } else {
        println!(
            "no mod artifact cache found for {}",
            _resolved.manifest.project.name
        );
    }
    Ok(())
}

struct ResolvedModProject {
    manifest: beskid_analysis::projects::ProjectManifest,
    plan: beskid_analysis::projects::CompilePlan,
}

fn mod_pipeline(plain: bool) -> Arc<CliPipeline> {
    Arc::new(CliPipeline::new_with_kind(
        use_cli_spinner(plain),
        PipelineProgressKind::ModBuild,
    ))
}

fn resolve_mod_project(
    project: Option<&PathBuf>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ResolvedModProject> {
    let manifest_path = observe_phase_result(pipeline, RESOLVE_MANIFEST, || {
        resolve_manifest_path(project)
    })?;
    let manifest = load_manifest_from_path(&manifest_path).map_err(anyhow::Error::from)?;
    if manifest.project.kind != ProjectKind::Mod {
        return Err(anyhow!(
            "`beskid mod` requires a `type = Mod` project, got `{}`",
            manifest.project.name
        ));
    }

    let plan = observe_phase_result(pipeline, RESOLVE_GRAPH, || {
        build_compile_plan(&manifest_path, None).map_err(anyhow::Error::from)
    })?;
    if !plan.unresolved_dependencies.is_empty() {
        let unresolved = plan
            .unresolved_dependencies
            .iter()
            .filter(|dependency| {
                dependency.source != beskid_analysis::projects::DependencySource::Registry
            })
            .map(|dependency| dependency.dependency_name.as_str())
            .collect::<Vec<_>>();
        if !unresolved.is_empty() {
            return Err(anyhow!(
                "unresolved mod project dependencies: {}",
                unresolved.join(", ")
            ));
        }
    }

    Ok(ResolvedModProject { manifest, plan })
}

fn resolve_manifest_path(project: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(project) = project {
        return resolve_explicit_project_path(project);
    }

    discover_project_manifest_from_input_or_cwd(None, None)?
        .map(|(manifest, _summary)| manifest)
        .ok_or_else(|| anyhow!("could not discover Project.proj from current directory"))
}

fn resolve_explicit_project_path(project: &Path) -> Result<PathBuf> {
    let candidate = if project.is_dir() {
        let project_manifest = project.join(PROJECT_FILE_NAME);
        if project_manifest.is_file() {
            project_manifest
        } else {
            project.join("Workspace.proj")
        }
    } else {
        project.to_path_buf()
    };

    if !candidate.is_file() {
        return Err(anyhow!(
            "project manifest not found at {}",
            candidate.display()
        ));
    }

    resolve_workspace_candidate_path(&candidate, None, None)
}

fn discover_mod_entry_source(source_root: &Path) -> Result<PathBuf> {
    for candidate in [
        source_root.join("__mod__.bd"),
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
        "could not infer mod entry source under {} (expected __mod__.bd, mod.bd, Main.bd, main.bd, lib.bd, or exactly one .bd file)",
        source_root.display()
    ))
}

fn remove_mod_cache_dir(project_root: &Path, package_id: &str) -> Result<bool> {
    let cache_dir = project_root
        .join(".beskid")
        .join("obj")
        .join("mods")
        .join(package_id);
    if !cache_dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&cache_dir).with_context(|| {
        format!(
            "failed to remove mod artifact cache {}",
            cache_dir.display()
        )
    })?;
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
        fs::write(&only_source, "unit main() { return; }\n").expect("source");

        assert_eq!(
            discover_mod_entry_source(&source_root).expect("entry source"),
            only_source
        );

        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{id}"))
    }
}
