//! Prepared test-workspace seam shared by single-target and matrix execution.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use beskid_analysis::projects::{
    CompilePlan, ProjectManifest, Target, TargetKind, load_manifest_from_path, plan_entry_path,
};
use beskid_analysis::services::ResolvedInput;
use beskid_tools::PipelineProgressKind;
use beskid_tools::session::{CommandSession, ResolveInputArgs};
use serde::{Deserialize, Serialize};

use super::test::TestArgs;

pub const DEFAULT_TARGET_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_MATRIX_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy)]
pub struct ExecutionBudgets {
    pub target: Duration,
    pub matrix: Duration,
}

impl Default for ExecutionBudgets {
    fn default() -> Self {
        Self { target: DEFAULT_TARGET_TIMEOUT, matrix: DEFAULT_MATRIX_TIMEOUT }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Cancellation {
    cancelled: Arc<AtomicBool>,
}

impl Cancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub head: String,
    pub content_revision: String,
    pub clean: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevisionSnapshot {
    pub root: Option<RepositorySnapshot>,
    pub compiler: Option<RepositorySnapshot>,
    pub corelib: Option<RepositorySnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetResult {
    Passed,
    Failed,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub phase: String,
    pub started_unix_ms: u64,
    pub ended_unix_ms: u64,
    pub duration_ms: u64,
    pub result: TargetResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetReport {
    pub target: String,
    pub started_unix_ms: u64,
    pub ended_unix_ms: u64,
    pub duration_ms: u64,
    pub active_phase: String,
    pub last_started_test: Option<String>,
    pub result: TargetResult,
    pub tests: super::test::TestSummary,
    pub phases: Vec<PhaseRecord>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerExitCause {
    Exited { code: i32 },
    Signaled { signal: i32 },
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixReport {
    pub manifest: PathBuf,
    pub revisions: RevisionSnapshot,
    pub denominator: usize,
    pub expected_targets: Vec<String>,
    pub selected: usize,
    pub filtered: bool,
    pub retried: bool,
    pub ignored: usize,
    pub skipped: usize,
    pub timed_out: bool,
    pub cancelled: bool,
    pub worker_exit_cause: Option<WorkerExitCause>,
    pub release_eligible: bool,
    pub targets: Vec<TargetReport>,
}

impl MatrixReport {
    pub fn finish_eligibility(&mut self, current: &RevisionSnapshot) {
        let revisions_complete = [&self.revisions.root, &self.revisions.compiler, &self.revisions.corelib]
            .into_iter()
            .all(|snapshot| snapshot.as_ref().is_some_and(|snapshot| snapshot.clean));
        let revisions_fresh = revisions_complete && self.revisions == *current;
        let all_passed = self.targets.iter().all(|target| target.result == TargetResult::Passed);
        let actual_targets = self.targets.iter().map(|target| target.target.as_str()).collect::<Vec<_>>();
        let expected_targets = self.expected_targets.iter().map(String::as_str).collect::<Vec<_>>();
        self.release_eligible = self.denominator == self.expected_targets.len()
            && actual_targets == expected_targets
            && self.selected == self.denominator
            && !self.filtered
            && !self.retried
            && self.ignored == 0
            && self.skipped == 0
            && !self.timed_out
            && !self.cancelled
            && self.worker_exit_cause == Some(WorkerExitCause::Exited { code: 0 })
            && revisions_fresh
            && self.targets.len() == self.denominator
            && all_passed;
    }
}

pub struct PreparedWorkspace {
    session: CommandSession,
    base: ResolvedInput,
    manifest: Option<ProjectManifest>,
    manifest_path: PathBuf,
    budgets: ExecutionBudgets,
    cancellation: Cancellation,
    started: Instant,
    revisions: RevisionSnapshot,
    /// Targets whose Mods the session's Mod host serves; only these may be prepared.
    mod_targets: Vec<String>,
}

pub struct PreparedTarget {
    pub name: String,
    pub resolved: ResolvedInput,
    pub front: beskid_analysis::services::ExecutableFrontEnd,
    pub tests: Vec<beskid_engine::services::SyntaxTestItem>,
}

impl PreparedWorkspace {
    pub fn prepare(args: &TestArgs, budgets: ExecutionBudgets, cancellation: Cancellation) -> Result<Self> {
        let resolve_args = ResolveInputArgs {
            input: args.input.as_ref(),
            project: args.project.project.as_ref(),
            target: args.project.target.as_deref(),
            workspace_member: args.project.workspace_member.as_deref(),
            frozen: args.lockfile.frozen,
            locked: args.lockfile.locked,
            offline: args.lockfile.offline,
        };
        let (mut session, base) =
            CommandSession::open_and_resolve(args.plain, PipelineProgressKind::PrepareAndRun, &resolve_args)?;
        let (manifest_path, manifest) =
            if let Some(plan) = base.compile_plan.as_ref().filter(|_| base.prepared_workspace.is_some()) {
                let manifest_path = plan.manifest_path.clone();
                let manifest = load_manifest_from_path(&manifest_path)
                    .map_err(|error| anyhow!("failed to load {}: {error}", manifest_path.display()))?;
                (manifest_path, Some(manifest))
            } else {
                (base.source_path.clone(), None)
            };
        let revisions = revision_snapshot(&manifest_path);
        Ok(Self {
            session,
            base,
            manifest,
            manifest_path,
            budgets,
            cancellation,
            started: Instant::now(),
            revisions,
            mod_targets: Vec::new(),
        })
    }

    /// Activate the Mods of exactly the targets this process will prepare. Each target is its own
    /// assembly and activates its own Mods; activation assembles every target it is given, so a
    /// worker or a single-target run must not pay for targets it never prepares.
    pub fn activate_mods(&mut self, targets: &[String], args: &TestArgs) -> Result<()> {
        let inputs = targets.iter().map(|name| self.target_input(name)).collect::<Result<Vec<_>>>()?;
        let mod_host = super::compiler_mod::prepare_native_mod_executor_for_inputs(
            &inputs,
            args.lockfile.WorkspaceOptions(),
            Some(self.session.observer()),
        )?;
        self.session.set_mod_host(mod_host);
        self.mod_targets = targets.to_vec();
        Ok(())
    }

    pub fn test_targets(&self) -> Vec<String> {
        match &self.manifest {
            Some(manifest) => manifest
                .targets
                .iter()
                .filter(|target| target.kind == TargetKind::Test || target.kind == TargetKind::Lib)
                .map(|target| target.name.clone())
                .collect(),
            None => vec!["direct-file".to_string()],
        }
    }

    pub fn prepare_targets(
        &self,
        names: &[String],
        mut target_started: impl FnMut(&str) -> Result<()>,
    ) -> Result<Vec<PreparedTarget>> {
        self.reject_mutation("prepare_targets")?;
        let mut targets = Vec::with_capacity(names.len());
        for name in names {
            target_started(name)?;
            targets.push(self.prepare_target(name)?);
        }
        self.reject_mutation("prepare_targets")?;
        Ok(targets)
    }

    fn prepare_target(&self, name: &str) -> Result<PreparedTarget> {
        self.check_budget(name, "prepare_target", None)?;
        if !self.mod_targets.iter().any(|target| target == name) {
            return Err(anyhow!("target `{name}` was not activated for Mods before preparation"));
        }
        let resolved = self.target_input(name)?;
        self.prepare_resolved_target(name.to_string(), resolved)
    }

    /// The resolved input of one matrix target: the base input for a direct file, otherwise the
    /// frozen manifest target's plan and entry over the base workspace.
    fn target_input(&self, name: &str) -> Result<ResolvedInput> {
        let Some(manifest) = &self.manifest else {
            if name != "direct-file" {
                return Err(anyhow!("direct-file input has no manifest target `{name}`"));
            }
            return Ok(self.base.clone());
        };
        let target = manifest.targets.iter().find(|target| target.name == name).cloned().ok_or_else(|| {
            anyhow!("target `{name}` is not present in frozen manifest {}", self.manifest_path.display())
        })?;
        let base_plan = self.base.compile_plan.as_ref().expect("manifest workspace has a compile plan");
        let plan = plan_for_target(base_plan, target);
        let source_root = self
            .base
            .prepared_workspace
            .as_ref()
            .map(|workspace| workspace.materialized_source_root.as_path())
            .unwrap_or(plan.source_root.as_path());
        let source_path = plan_entry_path(&plan, source_root);
        let source = if source_path.is_file() {
            std::fs::read_to_string(&source_path)
                .map_err(|error| anyhow!("failed to read {}: {error}", source_path.display()))?
        } else if plan.target.entry.as_deref().unwrap_or("").trim().is_empty() {
            String::new()
        } else {
            return Err(anyhow!("target `{name}` entry does not exist: {}", source_path.display()));
        };
        Ok(ResolvedInput {
            source_path,
            source,
            compile_plan: Some(plan),
            prepared_workspace: self.base.prepared_workspace.clone(),
            workspace_summary: self.base.workspace_summary.clone(),
            assembly: None,
        })
    }

    fn prepare_resolved_target(&self, name: String, resolved: ResolvedInput) -> Result<PreparedTarget> {
        let prepared = self.session.executable_gate_prepared(
            &resolved,
            beskid_tools::session::SemanticGateOptions {
                finish_prepare_ui: false,
                prepare_message: "Analysis complete",
            },
        )?;
        let tests = beskid_engine::services::syntax_test_items_from_front_end(prepared.executable())?;
        let front = prepared.into_executable()?;
        Ok(PreparedTarget { name, resolved, front, tests })
    }

    pub fn reject_mutation(&self, phase: &str) -> Result<()> {
        let current = revision_snapshot(&self.manifest_path);
        if current != self.revisions {
            return Err(anyhow!("prepared workspace content mutated during phase `{phase}`"));
        }
        Ok(())
    }

    pub fn session(&self) -> &CommandSession {
        &self.session
    }
    pub fn prepare_native_tests(
        &self,
        front: &beskid_analysis::services::ExecutableFrontEnd,
        resolved: &ResolvedInput,
        names: &[String],
        target_started: Instant,
    ) -> Result<NativeTests> {
        let directory = tempfile::tempdir()?;
        let target = beskid_engine::host_runtime_target().map_err(|error| anyhow!("{error}"))?;
        let cancellation = self.cancellation.clone();
        let control = beskid_aot::api::NativeExecutionControl::new(
            (target_started + self.budgets.target).min(self.started + self.budgets.matrix),
            Arc::new(move || cancellation.is_cancelled()),
        );
        control.check("lower_selected_tests")?;
        let prepared = beskid_pipeline::observe_phase_result(
            Some(self.session.observer()),
            beskid_pipeline::phases::CODEGEN_CLIF,
            || beskid_aot::prepared_syntax::lower_frozen_syntax_entrypoints(front, names, target),
        )?;
        control.check("lower_selected_tests")?;
        let libraries = beskid_engine::link_libraries::link_libraries_for_artifact(
            prepared.artifact(),
            resolved.compile_plan.as_ref(),
        );
        let profile = match std::env::var("BESKID_RUNTIME_KIT_PROFILE") {
            Ok(value) => match beskid_abi::runtime_kit::BuildProfile::parse(&value)? {
                beskid_abi::runtime_kit::BuildProfile::Debug => beskid_aot::BuildProfile::Debug,
                beskid_abi::runtime_kit::BuildProfile::Release => beskid_aot::BuildProfile::Release,
            },
            Err(std::env::VarError::NotPresent) => beskid_aot::BuildProfile::Debug,
            Err(error) => return Err(error.into()),
        };
        let kit = beskid_aot::default_runtime_strategy(profile, None)?;
        let object = beskid_aot::api::emit_native_test_object(
            prepared,
            directory.path(),
            profile,
            kit,
            libraries.external_libraries,
            libraries.library_search_paths,
            Some(self.session.observer()),
            control,
        )?;
        Ok(NativeTests { directory, object })
    }

    /// Links and executes `entries` (row index, qualified test name) on a bounded pool while
    /// `body` consumes their results in its own order through [`NativeTestRuns::take`]. Every test
    /// is still its own linked executable and process; the pool stops claiming new tests once
    /// `body` returns or the workspace is cancelled, and the shared deadline of the native object
    /// bounds the tests already running.
    pub fn with_native_test_runs<R>(
        &self,
        native: Option<&NativeTests>,
        entries: &[(usize, String)],
        body: impl FnOnce(&NativeTestRuns) -> R,
    ) -> R {
        let runs = NativeTestRuns {
            results: Mutex::new(HashMap::new()),
            ready: Condvar::new(),
            stop: AtomicBool::new(false),
            workers: AtomicUsize::new(0),
        };
        let Some(native) = native else {
            let mut results = runs.results.lock().unwrap_or_else(PoisonError::into_inner);
            for (row, name) in entries {
                results.insert(*row, (Err(anyhow!("native test `{name}` has no prepared native object")), Duration::ZERO));
            }
            drop(results);
            return body(&runs);
        };
        let observer = self.session.observer();
        let cancellation = &self.cancellation;
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let jobs = native_test_jobs().min(entries.len());
            runs.workers.store(jobs, Ordering::Release);
            for _ in 0..jobs {
                scope.spawn(|| {
                    while !runs.stop.load(Ordering::Acquire) && !cancellation.is_cancelled() {
                        let Some((row, name)) = entries.get(next.fetch_add(1, Ordering::AcqRel)) else { break };
                        let started = Instant::now();
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            native.run_entry(name, *row, observer)
                        }))
                        .unwrap_or_else(|_| Err(anyhow!("native test `{name}` runner panicked")));
                        runs.results.lock().unwrap_or_else(PoisonError::into_inner).insert(*row, (result, started.elapsed()));
                        runs.ready.notify_all();
                    }
                    // Wake a consumer waiting on a test no worker will claim any more.
                    let results = runs.results.lock().unwrap_or_else(PoisonError::into_inner);
                    runs.workers.fetch_sub(1, Ordering::AcqRel);
                    drop(results);
                    runs.ready.notify_all();
                });
            }
            struct StopOnExit<'a>(&'a NativeTestRuns);
            impl Drop for StopOnExit<'_> {
                fn drop(&mut self) {
                    self.0.stop.store(true, Ordering::Release);
                }
            }
            let _stop = StopOnExit(&runs);
            body(&runs)
        })
    }

    pub fn target_timeout(&self) -> Duration {
        self.budgets.target
    }
    pub fn revisions(&self) -> RevisionSnapshot {
        self.revisions.clone()
    }
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    pub fn check_budget(&self, target: &str, phase: &str, target_started: Option<Instant>) -> Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(anyhow!("matrix cancelled while target `{target}` was in phase `{phase}`"));
        }
        if self.started.elapsed() >= self.budgets.matrix {
            self.cancellation.cancel();
            return Err(anyhow!(
                "{}-second matrix budget expired while target `{target}` was in phase `{phase}`",
                self.budgets.matrix.as_secs()
            ));
        }
        if target_started.is_some_and(|started| started.elapsed() >= self.budgets.target) {
            self.cancellation.cancel();
            return Err(anyhow!(
                "{}-second target budget expired for `{target}` in phase `{phase}`",
                self.budgets.target.as_secs()
            ));
        }
        Ok(())
    }
}

fn plan_for_target(base: &CompilePlan, target: Target) -> CompilePlan {
    CompilePlan { target, ..base.clone() }
}

pub fn unix_ms() -> u64 {
    duration_ms(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default())
}

pub fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub fn revision_snapshot(manifest_path: &Path) -> RevisionSnapshot {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).map(Path::to_path_buf);
    let root = compiler.as_deref().and_then(git_superproject_root);
    RevisionSnapshot {
        root: root.as_deref().and_then(git_revision),
        compiler: compiler.as_deref().and_then(git_revision),
        corelib: manifest_path.parent().and_then(git_revision),
    }
}

fn git_superproject_root(compiler: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(compiler)
        .args(["rev-parse", "--show-superproject-working-tree"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout);
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn git_revision(path: &Path) -> Option<RepositorySnapshot> {
    let head = Command::new("git").arg("-C").arg(path).args(["rev-parse", "HEAD"]).output().ok()?;
    if !head.status.success() {
        return None;
    }
    let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
    if head.is_empty() {
        return None;
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"])
        .output()
        .ok()?;
    if !status.status.success() {
        return None;
    }
    let state = String::from_utf8_lossy(&status.stdout).into_owned();
    let diff =
        Command::new("git").arg("-C").arg(path).args(["diff", "--binary", "--no-ext-diff", "HEAD"]).output().ok()?;
    if !diff.status.success() {
        return None;
    }
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    head.hash(&mut identity);
    state.hash(&mut identity);
    diff.stdout.hash(&mut identity);
    let untracked =
        Command::new("git").arg("-C").arg(path).args(["ls-files", "--others", "--exclude-standard"]).output().ok()?;
    if !untracked.status.success() {
        return None;
    }
    for relative in String::from_utf8_lossy(&untracked.stdout).lines() {
        relative.hash(&mut identity);
        std::fs::read(path.join(relative)).ok()?.hash(&mut identity);
    }
    Some(RepositorySnapshot { content_revision: format!("{:016x}", identity.finish()), clean: state.is_empty(), head })
}

#[cfg(test)]
mod tests;

pub struct NativeTests {
    directory: tempfile::TempDir,
    object: beskid_aot::api::NativeTestObject,
}

impl NativeTests {
    /// Link one selected test into its own executable and run it in a fresh process.
    fn run_entry(&self, name: &str, index: usize, observer: &dyn beskid_pipeline::PipelineObserver) -> Result<String> {
        let directory = self.directory.path().join(format!("entry-{index}"));
        let executable =
            beskid_pipeline::observe_phase_result(Some(observer), beskid_pipeline::phases::AOT_LINK, || {
                self.object.link_entry(name, &directory)
            })?;
        let output = self.object.execute_linked_entry(&executable, &directory)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(anyhow!("native test `{name}` exited {}: {stdout}{stderr}", output.status));
        }
        Ok(format!("{stdout}{stderr}"))
    }
}

/// Results of [`PreparedWorkspace::with_native_test_runs`], keyed by row index.
pub struct NativeTestRuns {
    results: Mutex<HashMap<usize, (Result<String>, Duration)>>,
    ready: Condvar,
    stop: AtomicBool,
    workers: AtomicUsize,
}

impl NativeTestRuns {
    /// Block until the test of `row` has finished; returns its output and its link+run time.
    pub fn take(&self, row: usize) -> (Result<String>, Duration) {
        let mut results = self.results.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(result) = results.remove(&row) {
                return result;
            }
            if self.stop.load(Ordering::Acquire) || self.workers.load(Ordering::Acquire) == 0 {
                return (Err(anyhow!("native test runs stopped before row {row} ran")), Duration::ZERO);
            }
            results = self.ready.wait(results).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// `BESKID_NATIVE_TEST_JOBS` when set to a positive integer, otherwise up to four concurrent tests.
fn native_test_jobs() -> usize {
    std::env::var("BESKID_NATIVE_TEST_JOBS")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|jobs| *jobs > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |cores| cores.get().min(4)))
}
