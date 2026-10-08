//! Native Mod execution for consumers that never build Mods (the language server and editor
//! services).
//!
//! The CLI builds a missing or stale Mod artifact on demand. A non-building consumer instead
//! reuses an artifact that the CLI cached or that the toolchain shipped with its Corelib bundle,
//! and only when producer-side cache qualification proves the complete key tuple current
//! (`beskid_aot::qualify_cached_mod_artifact`). The Mod host executable is the installed `beskid`
//! CLI: it produced the artifact and it serves `beskid dev native-mod-worker`. When no current
//! artifact exists the consumer fails with the exact command that produces one; it never skips a
//! Mod silently.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use beskid_analysis::mod_host::ContractInvoker;
use beskid_analysis::projects::{CompilePlan, ProjectKind, build_compile_plan, load_manifest_from_path};

/// Executable names of the Mod host, in lookup order: the installed toolchain layout
/// (`<prefix>/bin/beskid`) and the Cargo target-directory layout (`target/<profile>/beskid_cli`).
const MOD_HOST_EXECUTABLE_STEMS: [&str; 2] = ["beskid", "beskid_cli"];

/// Locate the Mod host executable for the running process: the process itself when it is the
/// CLI (`beskid dev lsp`), otherwise the CLI installed next to it.
pub fn installed_mod_host_executable() -> Result<PathBuf> {
    let current = std::env::current_exe().context("locate the current executable")?;
    let stem = current.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    if MOD_HOST_EXECUTABLE_STEMS.contains(&stem) {
        return Ok(current);
    }
    let directory = current.parent().ok_or_else(|| anyhow!("current executable has no parent directory"))?;
    MOD_HOST_EXECUTABLE_STEMS
        .iter()
        .map(|stem| directory.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX)))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            anyhow!(
                "native Mods run in the `beskid` CLI, but no `beskid` executable is installed next to {}; \
                 install the complete Beskid toolchain",
                current.display()
            )
        })
}

/// Build a qualified Mod invoker for `plan` from cached or toolchain-shipped artifacts, using the
/// installed Mod host executable. `Ok(None)` when the plan has no Mod dependency.
pub fn installed_cached_mod_invoker(plan: &CompilePlan) -> Result<Option<Arc<dyn ContractInvoker>>> {
    if !has_mod_dependency(plan)? {
        return Ok(None);
    }
    cached_mod_invoker_for_plan(plan, &installed_mod_host_executable()?)
}

/// Build a qualified Mod invoker for every Mod dependency of `plan` from current cached or
/// toolchain-shipped artifacts produced by `mod_host_executable`. Fails with an actionable error
/// naming `beskid dev mod rebuild <manifest>` when any Mod lacks a current artifact.
pub fn cached_mod_invoker_for_plan(
    plan: &CompilePlan,
    mod_host_executable: &Path,
) -> Result<Option<Arc<dyn ContractInvoker>>> {
    let target = beskid_abi::runtime_kit::host_runtime_target()
        .map_err(|error| anyhow!("native Mod host target: {error}"))?;
    let prefix = beskid_abi::runtime_kit::installed_runtime_prefix()?;
    let profile = beskid_analysis::mod_host::native_mod_runtime_profile()?;
    let mut artifacts = Vec::new();
    for manifest_path in mod_dependency_manifests(plan)? {
        let manifest = load_manifest_from_path(&manifest_path).map_err(anyhow::Error::from)?;
        let mod_plan = build_compile_plan(&manifest_path, None).map_err(anyhow::Error::from)?;
        let request = beskid_aot::CachedModArtifactRequest {
            workspace_root: mod_plan.project_root.clone(),
            project_root: mod_plan.project_root.clone(),
            manifest_path: mod_plan.manifest_path.clone(),
            source_root: mod_plan.source_root.clone(),
            package_id: manifest.project.name.clone(),
            dependency_sources: beskid_analysis::mod_host::native_mod_dependency_sources(&mod_plan)?,
            compiler_version: None,
            compiler_executable: mod_host_executable.to_path_buf(),
            runtime: beskid_aot::api::RuntimeKitRequest { prefix: prefix.clone(), target: target.clone(), profile },
        };
        match beskid_aot::qualify_cached_mod_artifact(&request).map_err(anyhow::Error::from)? {
            Some(artifact) => artifacts.push(artifact),
            None => bail!(
                "no current native Mod artifact for {} built by {} for {} ({:?} runtime kit); \
                 run `beskid dev mod rebuild {}` (any `beskid build`, `run` or `check` of a dependent project \
                 also builds it){}",
                manifest.project.name,
                mod_host_executable.display(),
                target.triple.as_str(),
                profile,
                manifest_path.display(),
                if beskid_abi::corelib_bundle::verified_corelib_bundle_root(&manifest_path).is_some() {
                    "; this Mod belongs to the toolchain Corelib bundle, which ships a prebuilt artifact, so \
                     reinstalling the toolchain also restores it"
                } else {
                    ""
                }
            ),
        }
    }
    if artifacts.is_empty() {
        return Ok(None);
    }
    let control = beskid_aot::api::NativeExecutionControl::new(
        std::time::Instant::now() + std::time::Duration::from_secs(300),
        Arc::new(|| false),
    );
    let invoker = beskid_aot::QualifiedModInvoker::new(artifacts, mod_host_executable.to_path_buf(), control)
        .map_err(anyhow::Error::from)?;
    Ok(Some(Arc::new(invoker)))
}

fn has_mod_dependency(plan: &CompilePlan) -> Result<bool> {
    Ok(!mod_dependency_manifests(plan)?.is_empty())
}

/// Canonical manifests of the plan's `type = Mod` dependency projects, deduplicated, in stable order.
fn mod_dependency_manifests(plan: &CompilePlan) -> Result<Vec<PathBuf>> {
    let mut manifests = BTreeSet::new();
    for dependency in &plan.dependency_projects {
        let manifest_path = dependency
            .manifest_path
            .canonicalize()
            .with_context(|| format!("resolve dependency manifest {}", dependency.manifest_path.display()))?;
        if manifests.contains(&manifest_path) {
            continue;
        }
        let manifest = load_manifest_from_path(&manifest_path).map_err(anyhow::Error::from)?;
        if manifest.project.kind == ProjectKind::Mod {
            manifests.insert(manifest_path);
        }
    }
    Ok(manifests.into_iter().collect())
}
