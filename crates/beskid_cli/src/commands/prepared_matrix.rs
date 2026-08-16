//! Prepared test-workspace seam shared by single-target and matrix execution.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use beskid_analysis::projects::{
    CompilePlan, ProjectManifest, Target, TargetKind, load_manifest_from_path, plan_entry_path,
};
use beskid_analysis::services::ResolvedInput;
use beskid_engine::Engine;
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_tools::PipelineProgressKind;
use beskid_tools::session::{CommandSession, ResolveInputArgs};
use beskid_tools::tui::shell::runtime::RuntimeOp;
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
    pub started_unix_ms: u128,
    pub ended_unix_ms: u128,
    pub duration_ms: u128,
    pub result: TargetResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetReport {
    pub target: String,
    pub started_unix_ms: u128,
    pub ended_unix_ms: u128,
    pub duration_ms: u128,
    pub active_phase: String,
    pub result: TargetResult,
    pub tests: super::test::TestSummary,
    pub phases: Vec<PhaseRecord>,
    pub error: Option<String>,
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
    engine: Engine,
    budgets: ExecutionBudgets,
    cancellation: Cancellation,
    started: Instant,
    revisions: RevisionSnapshot,
}

pub struct PreparedTarget {
    pub name: String,
    pub resolved: ResolvedInput,
    pub front: beskid_analysis::services::FrontEndTypedResult,
    pub tests: Vec<beskid_engine::services::SyntaxTestItem>,
}

impl PreparedWorkspace {
    pub fn prepare(
        args: &TestArgs,
        hi_tx: Option<Sender<RuntimeOp>>,
        budgets: ExecutionBudgets,
        cancellation: Cancellation,
    ) -> Result<Self> {
        let resolve_args = ResolveInputArgs {
            input: args.input.as_ref(),
            project: args.project.project.as_ref(),
            target: args.project.target.as_deref(),
            workspace_member: args.project.workspace_member.as_deref(),
            frozen: args.lockfile.frozen,
            locked: args.lockfile.locked,
        };
        let (session, base) = match hi_tx {
            None => CommandSession::open_and_resolve(args.plain, PipelineProgressKind::PrepareAndRun, &resolve_args)?,
            Some(tx) => {
                let session = CommandSession::with_attached_pipeline(tx, PipelineProgressKind::PrepareAndRun);
                let resolved = session.resolve_input(&resolve_args)?;
                (session, resolved)
            }
        };
        let (manifest_path, manifest) = if let Some(plan) = base.compile_plan.as_ref() {
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
            engine: Engine::try_new()
                .map_err(|error| anyhow!("failed to initialize exact ABI-v5 runtime kit: {error}"))?,
            budgets,
            cancellation,
            started: Instant::now(),
            revisions,
        })
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
        let Some(manifest) = &self.manifest else {
            if name != "direct-file" {
                return Err(anyhow!("direct-file input has no manifest target `{name}`"));
            }
            return self.prepare_resolved_target(name.to_string(), self.base.clone());
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
        self.prepare_resolved_target(
            name.to_string(),
            ResolvedInput {
                source_path,
                source,
                compile_plan: Some(plan),
                prepared_workspace: self.base.prepared_workspace.clone(),
                workspace_summary: self.base.workspace_summary.clone(),
                assembly: None,
            },
        )
    }

    fn prepare_resolved_target(&self, name: String, resolved: ResolvedInput) -> Result<PreparedTarget> {
        let prepared = self.session.executable_gate_prepared(
            &resolved,
            beskid_tools::session::SemanticGateOptions {
                finish_prepare_ui: false,
                prepare_message: "Analysis complete",
            },
        )?;
        let tests = beskid_engine::services::syntax_test_items_from_front_end(prepared.executable()?)?;
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
    pub fn run_entrypoint(
        &mut self,
        front: &beskid_analysis::services::FrontEndTypedResult,
        source_name: &str,
        source: &str,
        qualified_name: &str,
    ) -> Result<String> {
        run_entrypoint_from_front_end_with_engine(
            &mut self.engine,
            front,
            source_name,
            source,
            qualified_name,
            Some(self.session.observer()),
        )
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
            return Err(anyhow!("30-minute matrix budget expired while target `{target}` was in phase `{phase}`"));
        }
        if target_started.is_some_and(|started| started.elapsed() >= self.budgets.target) {
            self.cancellation.cancel();
            return Err(anyhow!("120-second target budget expired for `{target}` in phase `{phase}`"));
        }
        Ok(())
    }
}

fn plan_for_target(base: &CompilePlan, target: Target) -> CompilePlan {
    CompilePlan { target, ..base.clone() }
}

pub fn unix_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

pub fn revision_snapshot(manifest_path: &Path) -> RevisionSnapshot {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).map(Path::to_path_buf);
    let root = compiler.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    RevisionSnapshot {
        root: root.as_deref().and_then(git_revision),
        compiler: compiler.as_deref().and_then(git_revision),
        corelib: manifest_path.parent().and_then(git_revision),
    }
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
