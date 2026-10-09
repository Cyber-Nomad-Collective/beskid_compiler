//! Native Mod host for in-process tests that prepare Corelib-dependent projects.
//!
//! Corelib depends on the native Serialization Mod, so preparing any Corelib project runs that
//! Mod's Collector. In-process analysis and compilation are non-building consumers, like the
//! language server: they reuse a Mod artifact only when producer-side qualification proves it
//! current for the Mod host that produced it. The Mod host is the `beskid` CLI (it builds the
//! artifact and serves `beskid dev native-mod-worker`). This harness therefore:
//!
//! - stages one exact debug runtime kit per target directory and canonical runtime source hash
//!   ([`shared_runtime_prefix`]);
//! - locates the current CLI ([`provisioned_cli`]);
//! - provisions Mod artifacts with that CLI (`beskid dev mod rebuild`, as release bundles and
//!   `beskid build` do) when no current artifact exists, and returns the qualified invoker
//!   ([`corelib_mod_invoker`], [`provision_corelib_mods`]), or, in a process that runs from a
//!   staged installation prefix, against the runtime kit that process resolves
//!   ([`installed_runtime_mod_invoker`]).
//!
//! It never skips a Mod and never substitutes an unqualified invoker; every failure is an error.
//!
//! Every value is computed once per process. Work shared between parallel test processes is
//! serialized with exclusive OS file locks (the runtime kit under the Cargo profile directory, Mod
//! provisioning machine-wide, because Mod artifacts live in the shared Corelib tree), so
//! concurrent processes wait for, and then reuse, one publication.
//!
//! Mod qualification and Corelib preparation read `BESKID_RUNTIME_PREFIX` and
//! `BESKID_RUNTIME_KIT_PROFILE`; hold a [`ModHostScope`] across qualification and preparation.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use anyhow::{Context, Result, anyhow, bail};
use beskid_abi::runtime_kit::BuildProfile;
use beskid_abi::runtime_source::{canonical_runtime_source_hash, resolve_canonical_runtime_kit};
use beskid_analysis::mod_host::ContractInvoker;
use beskid_analysis::projects::{CompilePlan, ProjectKind, load_manifest_from_path};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

const ENV_RUNTIME_PREFIX: &str = "BESKID_RUNTIME_PREFIX";
const ENV_RUNTIME_KIT_PROFILE: &str = "BESKID_RUNTIME_KIT_PROFILE";
const ENV_CLI_BIN: &str = "BESKID_CLI_BIN";
/// The shared kit's profile, as `BESKID_RUNTIME_KIT_PROFILE` spells it.
const KIT_PROFILE: &str = "debug";
const CLI_PACKAGE: &str = "beskid_cli";

static CLI_BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
static RUNTIME_PREFIX: OnceLock<Result<PathBuf, String>> = OnceLock::new();
static PROVISIONED_CORELIBS: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
static SCOPE_LOCK: Mutex<()> = Mutex::new(());

/// The debug runtime kit shared by every test of this target directory and by the Mods it
/// provisions.
///
/// Kept under the Cargo profile directory of the running test executable and keyed by the
/// canonical runtime source hash, so a Mod artifact bound to this kit stays current across runs
/// and a changed runtime gets a fresh kit. Staging is serialized across processes.
pub fn shared_runtime_prefix() -> Result<PathBuf> {
    cached(&RUNTIME_PREFIX, stage_runtime_prefix)
}

/// The Mod host executable: `BESKID_CLI_BIN` when the caller supplies one, otherwise the
/// workspace CLI built through Cargo, which owns its freshness.
pub fn provisioned_cli() -> Result<PathBuf> {
    cached(&CLI_BINARY, locate_cli)
}

/// Provisions every native Mod of the Corelib workspace at `corelib_root` (each Mod project under
/// `<corelib_root>/mods`) with [`provisioned_cli`] against [`shared_runtime_prefix`].
///
/// Use it for sealed Corelib copies, whose Mod artifacts live inside the copy. Runs once per
/// root and process.
pub fn provision_corelib_mods(corelib_root: &Path) -> Result<()> {
    let root = corelib_root
        .canonicalize()
        .with_context(|| format!("resolve Corelib root {}", corelib_root.display()))?;
    let mut provisioned = PROVISIONED_CORELIBS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if provisioned.contains(&root) {
        return Ok(());
    }
    let prefix = shared_runtime_prefix()?;
    let cli = provisioned_cli()?;
    let manifests = corelib_mod_manifests(&root)?;
    let _lock = FileLock::acquire(&provisioning_lock_path())?;
    for manifest in &manifests {
        rebuild_mod(&cli, manifest, &prefix, BuildProfile::Debug)?;
    }
    provisioned.insert(root);
    Ok(())
}

/// A qualified invoker for every native Mod dependency of `plan` against [`shared_runtime_prefix`],
/// provisioning missing or stale artifacts with [`provisioned_cli`].
///
/// Requires an entered [`ModHostScope`]: qualification and the later preparation resolve the
/// runtime kit from the process environment. Fails when `plan` has no Mod dependency.
pub fn corelib_mod_invoker(plan: &CompilePlan) -> Result<Arc<dyn ContractInvoker>> {
    let prefix = shared_runtime_prefix()?;
    require_scope(&prefix)?;
    qualified_invoker(plan, &prefix, BuildProfile::Debug)
}

/// A qualified invoker for every native Mod dependency of `plan` against the runtime kit this
/// process resolves itself (`BESKID_RUNTIME_PREFIX`, else the installation prefix of the running
/// executable, with `BESKID_RUNTIME_KIT_PROFILE`), provisioning missing or stale artifacts with
/// [`provisioned_cli`] for that same kit.
///
/// Use it in processes that run from a staged installation prefix instead of a Cargo build tree.
pub fn installed_runtime_mod_invoker(plan: &CompilePlan) -> Result<Arc<dyn ContractInvoker>> {
    let prefix = beskid_abi::runtime_kit::installed_runtime_prefix()
        .map_err(|error| anyhow!("resolve the installed runtime prefix: {error}"))?;
    let profile = beskid_analysis::mod_host::native_mod_runtime_profile()?;
    qualified_invoker(plan, &prefix, profile)
}

fn qualified_invoker(plan: &CompilePlan, prefix: &Path, profile: BuildProfile) -> Result<Arc<dyn ContractInvoker>> {
    let cli = provisioned_cli()?;
    let manifests = mod_manifests(plan)?;
    if manifests.is_empty() {
        bail!("compile plan {} has no native Mod dependency", plan.manifest_path.display());
    }
    if let Ok(Some(invoker)) = qualify(plan, &cli) {
        return Ok(invoker);
    }
    // Another process may be provisioning the same artifacts; wait for it and re-qualify before
    // rebuilding.
    let _lock = FileLock::acquire(&provisioning_lock_path())?;
    if let Ok(Some(invoker)) = qualify(plan, &cli) {
        return Ok(invoker);
    }
    for manifest in &manifests {
        rebuild_mod(&cli, manifest, prefix, profile)?;
    }
    qualify(plan, &cli)
        .context("qualify provisioned native Mods")?
        .ok_or_else(|| anyhow!("compile plan {} has no native Mod dependency", plan.manifest_path.display()))
}

/// Process-wide scope that points the runtime-kit environment at [`shared_runtime_prefix`].
///
/// `BESKID_RUNTIME_PREFIX` and `BESKID_RUNTIME_KIT_PROFILE` are process-global, while libtest
/// runs cases of one binary concurrently. The scope holds a process-wide lock from the mutation
/// through qualification and preparation and restores the previous values on drop. Do not enter
/// it again on a thread that already holds it.
pub struct ModHostScope {
    prefix: PathBuf,
    previous_prefix: Option<OsString>,
    previous_profile: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl ModHostScope {
    pub fn enter() -> Result<Self> {
        let prefix = shared_runtime_prefix()?;
        let lock = SCOPE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_prefix = std::env::var_os(ENV_RUNTIME_PREFIX);
        let previous_profile = std::env::var_os(ENV_RUNTIME_KIT_PROFILE);
        // SAFETY: every mutation of these variables in this process goes through this scope, and
        // the process-wide lock is held until Drop restores the exact previous values.
        unsafe {
            std::env::set_var(ENV_RUNTIME_PREFIX, &prefix);
            std::env::set_var(ENV_RUNTIME_KIT_PROFILE, KIT_PROFILE);
        }
        Ok(Self { prefix, previous_prefix, previous_profile, _lock: lock })
    }

    /// The exact runtime kit prefix this scope installed.
    pub fn runtime_prefix(&self) -> &Path {
        &self.prefix
    }

    /// [`corelib_mod_invoker`] under this scope.
    pub fn corelib_mod_invoker(&self, plan: &CompilePlan) -> Result<Arc<dyn ContractInvoker>> {
        corelib_mod_invoker(plan)
    }
}

impl Drop for ModHostScope {
    fn drop(&mut self) {
        // SAFETY: the scope still holds the process-wide lock taken in `enter`.
        unsafe {
            restore(ENV_RUNTIME_PREFIX, self.previous_prefix.take());
            restore(ENV_RUNTIME_KIT_PROFILE, self.previous_profile.take());
        }
    }
}

/// # Safety
/// The caller holds [`SCOPE_LOCK`].
unsafe fn restore(name: &str, value: Option<OsString>) {
    match value {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}

fn require_scope(prefix: &Path) -> Result<()> {
    let current_prefix = std::env::var_os(ENV_RUNTIME_PREFIX);
    let current_profile = std::env::var_os(ENV_RUNTIME_KIT_PROFILE);
    if current_prefix.as_deref() != Some(prefix.as_os_str()) || current_profile.as_deref() != Some(OsStr::new(KIT_PROFILE))
    {
        bail!(
            "native Mod qualification requires an entered ModHostScope ({ENV_RUNTIME_PREFIX}={}, \
             {ENV_RUNTIME_KIT_PROFILE}={KIT_PROFILE}); found {ENV_RUNTIME_PREFIX}={current_prefix:?}, \
             {ENV_RUNTIME_KIT_PROFILE}={current_profile:?}",
            prefix.display()
        );
    }
    Ok(())
}

fn qualify(plan: &CompilePlan, cli: &Path) -> Result<Option<Arc<dyn ContractInvoker>>> {
    beskid_tools::native_mods::cached_mod_invoker_for_plan(plan, cli)
}

fn cached(cell: &'static OnceLock<Result<PathBuf, String>>, init: fn() -> Result<PathBuf>) -> Result<PathBuf> {
    cell.get_or_init(|| init().map_err(|error| format!("{error:#}"))).clone().map_err(|error| anyhow!(error))
}

/// `<target>/<profile>/beskid-test-mod-host`, derived from the running test executable, which
/// Cargo places in `<target>/<profile>/deps`.
fn harness_root() -> Result<PathBuf> {
    let executable = std::env::current_exe().context("locate the running test executable")?;
    let deps = executable
        .parent()
        .filter(|directory| directory.file_name() == Some(OsStr::new("deps")))
        .ok_or_else(|| {
            anyhow!("test executable {} is not inside a Cargo `<profile>/deps` directory", executable.display())
        })?;
    let profile = deps
        .parent()
        .ok_or_else(|| anyhow!("Cargo deps directory {} has no profile directory", deps.display()))?;
    Ok(profile.join("beskid-test-mod-host"))
}

fn stage_runtime_prefix() -> Result<PathBuf> {
    let root = harness_root()?.join("runtime-kits");
    let hash = canonical_runtime_source_hash();
    let prefix = root.join(&hash);
    let target = beskid_abi::runtime_kit::host_runtime_target()
        .map_err(|error| anyhow!("supported native host target: {error:?}"))?;
    let _lock = FileLock::acquire(&root.join(format!("{hash}.lock")))?;
    if resolve_canonical_runtime_kit(&prefix, &target, BuildProfile::Debug).is_ok() {
        return Ok(prefix);
    }
    if prefix.exists() {
        fs::remove_dir_all(&prefix).with_context(|| format!("remove incomplete runtime kit {}", prefix.display()))?;
    }
    build_native_host(prefix.clone(), RuntimeKitProfile::Debug)
        .with_context(|| format!("publish exact native runtime kit {}", prefix.display()))?;
    resolve_canonical_runtime_kit(&prefix, &target, BuildProfile::Debug)
        .map_err(|error| anyhow!("staged runtime kit {} is not exact: {error:?}", prefix.display()))?;
    Ok(prefix)
}

fn locate_cli() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(ENV_CLI_BIN) {
        let binary = PathBuf::from(path);
        if !binary.is_file() {
            bail!("{ENV_CLI_BIN} points to a missing file: {}", binary.display());
        }
        return Ok(binary);
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .current_dir(&workspace)
        .args(["build", "--locked", "-p", CLI_PACKAGE, "--bin", CLI_PACKAGE, "--message-format=json-render-diagnostics"])
        .output()
        .context("invoke `cargo build -p beskid_cli`")?;
    if !output.status.success() {
        bail!("build the Mod host CLI\nstderr:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    let stdout = String::from_utf8(output.stdout).context("Cargo build messages are UTF-8")?;
    let mut executable = None;
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let message: serde_json::Value =
            serde_json::from_str(line).with_context(|| format!("parse Cargo build message `{line}`"))?;
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == CLI_PACKAGE
            && let Some(path) = message["executable"].as_str()
        {
            executable = Some(PathBuf::from(path));
        }
    }
    let binary = executable.ok_or_else(|| anyhow!("Cargo reported no `{CLI_PACKAGE}` executable"))?;
    if !binary.is_file() {
        bail!("Mod host CLI missing after build: {}", binary.display());
    }
    Ok(binary)
}

fn mod_manifests(plan: &CompilePlan) -> Result<Vec<PathBuf>> {
    let mut manifests = BTreeSet::new();
    for dependency in &plan.dependency_projects {
        let manifest_path = dependency
            .manifest_path
            .canonicalize()
            .with_context(|| format!("resolve {}", dependency.manifest_path.display()))?;
        if is_mod_manifest(&manifest_path)? {
            manifests.insert(manifest_path);
        }
    }
    Ok(manifests.into_iter().collect())
}

/// Every Mod project manifest directly under `<corelib_root>/mods/<project>/`.
fn corelib_mod_manifests(corelib_root: &Path) -> Result<Vec<PathBuf>> {
    let mods = corelib_root.join("mods");
    let mut manifests = BTreeSet::new();
    for project in fs::read_dir(&mods).with_context(|| format!("list Corelib Mods in {}", mods.display()))? {
        let project = project.with_context(|| format!("list Corelib Mods in {}", mods.display()))?.path();
        if !project.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&project).with_context(|| format!("list {}", project.display()))? {
            let path = entry.with_context(|| format!("list {}", project.display()))?.path();
            if path.extension().is_some_and(|extension| extension == "bproj") && is_mod_manifest(&path)? {
                manifests.insert(path);
            }
        }
    }
    if manifests.is_empty() {
        bail!("Corelib workspace {} declares no native Mod under {}", corelib_root.display(), mods.display());
    }
    Ok(manifests.into_iter().collect())
}

fn is_mod_manifest(manifest_path: &Path) -> Result<bool> {
    let manifest = load_manifest_from_path(manifest_path)
        .map_err(|error| anyhow!("load {}: {error}", manifest_path.display()))?;
    Ok(manifest.project.kind == ProjectKind::Mod)
}

/// One machine-wide lock serializes Mod provisioning by every test process: Mod artifacts live
/// in the shared Corelib source tree, whichever target directory a test runs from.
fn provisioning_lock_path() -> PathBuf {
    std::env::temp_dir().join("beskid-test-mod-host").join("mod-provisioning.lock")
}

fn profile_name(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    }
}

fn rebuild_mod(cli: &Path, manifest: &Path, prefix: &Path, profile: BuildProfile) -> Result<()> {
    let output = Command::new(cli)
        .args(["dev", "mod", "rebuild"])
        .arg(manifest)
        .args(["--offline", "--plain"])
        .env(ENV_RUNTIME_PREFIX, prefix)
        .env(ENV_RUNTIME_KIT_PROFILE, profile_name(profile))
        .output()
        .with_context(|| format!("invoke `{} dev mod rebuild`", cli.display()))?;
    if !output.status.success() {
        bail!(
            "provision native Mod {}\nstdout:\n{}\nstderr:\n{}",
            manifest.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Exclusive OS file lock (`flock` / `LockFileEx`), released on drop or process exit.
struct FileLock(File);

impl FileLock {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("open lock {}", path.display()))?;
        file.lock().with_context(|| format!("lock {}", path.display()))?;
        Ok(Self(file))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
