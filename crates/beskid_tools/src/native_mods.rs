//! Native Mod execution for consumers that never build Mods (the language server and editor
//! services).
//!
//! The CLI builds a missing or stale Mod artifact on demand. A non-building consumer instead
//! reuses an artifact that the CLI cached or that the toolchain shipped with its Corelib bundle,
//! and only when producer-side cache qualification proves the complete key tuple current
//! (`beskid_aot::qualify_cached_mod_artifact`). The Mod host executable is the installed `beskid`
//! CLI unless the consumer selects another one ([`ModHostSelection`]): it produced the artifact and
//! it serves `beskid dev native-mod-worker`. When no current
//! artifact exists the consumer fails with the exact command that produces one; it never skips a
//! Mod silently.
//!
//! Only the Mods the input activates (`beskid_analysis::mod_host::activation`) are qualified: a Mod
//! of a package the program never reaches needs no artifact, exactly as in the CLI and the prepare
//! spine.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use beskid_analysis::mod_host::{ModHost, RuntimeKitSelection};
use beskid_analysis::projects::{CompilePlan, build_compile_plan, load_manifest_from_path, plan_entry_path};
use beskid_analysis::services::{FrontEndOptions, ResolvedInput, resolved_input_from_plan};

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

/// The Mod host a non-building consumer runs native Mods in: the executable that produced the
/// artifacts and serves `beskid dev native-mod-worker`, and the exact runtime kit the artifacts are
/// qualified and loaded against.
///
/// The default selection is the installed toolchain's CLI ([`installed_mod_host_executable`]) and
/// the runtime kit this process resolves ([`RuntimeKitSelection::installed`]). An explicit
/// selection (an editor setting, in the manner of rust-analyzer's `procMacro.server`) names
/// another executable, another runtime kit, or both; each is validated when it is made and never
/// bypasses artifact qualification.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModHostSelection {
    executable: Option<PathBuf>,
    runtime: Option<RuntimeKitSelection>,
}

impl ModHostSelection {
    /// The installed toolchain's Mod host and runtime kit.
    pub fn installed() -> Self {
        Self::default()
    }

    /// An explicit Mod host: `executable` must be an absolute path to an existing executable file.
    pub fn explicit(executable: &Path) -> Result<Self> {
        Ok(Self { executable: Some(validated_mod_host_executable(executable)?), runtime: None })
    }

    /// This selection with an explicit runtime kit in place of the installed one.
    pub fn with_runtime_kit(self, runtime: RuntimeKitSelection) -> Self {
        Self { runtime: Some(runtime), ..self }
    }

    /// The explicit Mod host executable, if any.
    pub fn executable(&self) -> Option<&Path> {
        self.executable.as_deref()
    }

    /// The explicit runtime kit, if any.
    pub fn runtime_kit(&self) -> Option<&RuntimeKitSelection> {
        self.runtime.as_ref()
    }

    /// The native Mod host for `plan` (prepared from its own entry with the default assembly policy)
    /// from cached or toolchain-shipped artifacts produced by this Mod host for this runtime kit.
    pub fn mod_host_for_plan(&self, plan: &CompilePlan) -> Result<ModHost> {
        let active = active_mod_manifests_for_plan(plan)?;
        self.mod_host_for_active(&active)
    }

    /// The native Mod host for the Mods `resolved` activates when it is prepared with `front_end`,
    /// from cached or toolchain-shipped artifacts produced by this Mod host for this runtime kit:
    /// [`ModHost::NoMods`] when it activates none. The installed host and kit are resolved only when
    /// a Mod must run.
    pub fn mod_host_for_input(&self, resolved: &ResolvedInput, front_end: &FrontEndOptions) -> Result<ModHost> {
        let active = beskid_analysis::mod_host::active_mod_manifests_for_input(resolved, front_end, false)?;
        self.mod_host_for_active(&active)
    }

    fn mod_host_for_active(&self, active: &[PathBuf]) -> Result<ModHost> {
        if active.is_empty() {
            return Ok(ModHost::NoMods);
        }
        let runtime = match &self.runtime {
            Some(runtime) => runtime.clone(),
            None => RuntimeKitSelection::installed()?,
        };
        match &self.executable {
            Some(executable) => qualified_mod_host(active, executable, runtime),
            None => qualified_mod_host(active, &installed_mod_host_executable()?, runtime),
        }
    }
}

/// Canonicalize `path` and require an executable regular file.
fn validated_mod_host_executable(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("native Mod host executable {} is not an absolute path", path.display());
    }
    let canonical =
        path.canonicalize().with_context(|| format!("resolve native Mod host executable {}", path.display()))?;
    let metadata = std::fs::metadata(&canonical)
        .with_context(|| format!("inspect native Mod host executable {}", canonical.display()))?;
    if !metadata.is_file() {
        bail!("native Mod host {} is not a file", canonical.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("native Mod host {} is not executable", canonical.display());
        }
    }
    Ok(canonical)
}

/// The native Mod host for the Mods `plan` activates ([`active_mod_manifests_for_plan`]) from current cached or toolchain-shipped artifacts produced by
/// `mod_host_executable` for `runtime`: [`ModHost::NoMods`] when it activates none. Fails with an
/// actionable error naming `beskid dev mod rebuild <manifest>` when an active Mod lacks a current
/// artifact.
pub fn cached_mod_host_for_plan(
    plan: &CompilePlan,
    mod_host_executable: &Path,
    runtime: &RuntimeKitSelection,
) -> Result<ModHost> {
    let active = active_mod_manifests_for_plan(plan)?;
    if active.is_empty() {
        return Ok(ModHost::NoMods);
    }
    qualified_mod_host(&active, mod_host_executable, runtime.clone())
}

/// Canonical manifests of the Mods `plan` activates when prepared from its own entry with the
/// default assembly policy (a Mod project's plan with the SDK adapter sources its build adds).
pub fn active_mod_manifests_for_plan(plan: &CompilePlan) -> Result<Vec<PathBuf>> {
    let native_mod_adapter_sources = beskid_analysis::services::is_mod_compile_plan(Some(plan));
    beskid_analysis::mod_host::active_mod_manifests_for_input(
        &plan_entry_input(plan)?,
        &FrontEndOptions::default(),
        native_mod_adapter_sources,
    )
}

/// `plan` with the source of its own target entry (empty for an entry-less library).
fn plan_entry_input(plan: &CompilePlan) -> Result<ResolvedInput> {
    let entry = plan_entry_path(plan, &plan.source_root);
    let source = if entry.is_file() {
        std::fs::read_to_string(&entry).with_context(|| format!("read entry {}", entry.display()))?
    } else {
        String::new()
    };
    Ok(resolved_input_from_plan(entry, source, plan.clone(), None, None))
}

fn qualified_mod_host(
    manifests: &[PathBuf],
    mod_host_executable: &Path,
    runtime: RuntimeKitSelection,
) -> Result<ModHost> {
    let target = beskid_abi::runtime_kit::host_runtime_target()
        .map_err(|error| anyhow!("native Mod host target: {error}"))?;
    let prefix = runtime.prefix().to_path_buf();
    let profile = runtime.profile();
    let mut artifacts = Vec::new();
    for manifest_path in manifests {
        let manifest = load_manifest_from_path(manifest_path).map_err(anyhow::Error::from)?;
        let mod_plan = build_compile_plan(manifest_path, None).map_err(anyhow::Error::from)?;
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
                if beskid_abi::corelib_bundle::verified_corelib_bundle_root(manifest_path).is_some() {
                    "; this Mod belongs to the toolchain Corelib bundle, which ships a prebuilt artifact, so \
                     reinstalling the toolchain also restores it"
                } else {
                    ""
                }
            ),
        }
    }
    if artifacts.is_empty() {
        bail!("no active native Mod to qualify");
    }
    let invoker = beskid_aot::QualifiedModInvoker::new(
        artifacts,
        mod_host_executable.to_path_buf(),
        std::time::Duration::from_secs(300),
        Arc::new(|| false),
    )
        .map_err(anyhow::Error::from)?;
    Ok(ModHost::native(Arc::new(invoker), runtime))
}

