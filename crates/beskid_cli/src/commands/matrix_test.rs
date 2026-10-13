//! Corelib matrix presentation over isolated prepared-workspace shard workers.
//!
//! The supervisor splits the selected targets round-robin over a fixed number of worker
//! processes. Each worker prepares the workspace once and runs only its shard; the supervisor
//! merges the per-target reports back into manifest target order, so the report and the progress
//! lines do not depend on which worker finished first.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use super::prepared_matrix::{
    Cancellation, MatrixReport, PreparedWorkspace, RevisionSnapshot, TargetReport, TargetResult, WorkerExitCause,
    duration_ms, unix_ms,
};
use super::test::{TestArgs, TestSummary, execute_prepared_target};

const MATRIX_WORKER_ENV: &str = "BESKID_PREPARED_MATRIX_WORKER";
const MATRIX_CANCEL_LOCK_ENV: &str = "BESKID_PREPARED_MATRIX_CANCEL_LOCK";
/// `<index>/<count>`: the shard of the selected targets a worker prepares and runs.
const MATRIX_SHARD_ENV: &str = "BESKID_PREPARED_MATRIX_SHARD";
/// Optional positive worker count; the default is a quarter of the available cores, at most four.
const MATRIX_WORKERS_ENV: &str = "BESKID_MATRIX_WORKERS";
const MAX_DEFAULT_WORKERS: usize = 4;
const WORKER_EVENT_PREFIX: &str = "\u{1e}";
const SUPERVISOR_POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum WorkerEvent {
    Prepared {
        manifest: std::path::PathBuf,
        revisions: RevisionSnapshot,
        expected_targets: Vec<String>,
        selected_targets: Vec<String>,
        shard_targets: Vec<String>,
        filtered: bool,
    },
    TargetStarted {
        target: String,
        phase: String,
    },
    TestStarted {
        target: String,
        test: String,
    },
    TargetFinished {
        report: TargetReport,
    },
    Fatal {
        phase: String,
        error: String,
    },
}

#[derive(Clone)]
struct ActiveTarget {
    target: String,
    phase: String,
    last_started_test: Option<String>,
    started: Instant,
}

pub fn execute_all_targets(args: TestArgs) -> Result<()> {
    if env::var_os(MATRIX_WORKER_ENV).is_some() {
        return execute_worker(args);
    }
    supervise_worker(args)
}

/// Blocks until the supervisor releases the cancellation lock. Failing to
/// open the lock (missing path, removed file) is treated as a stop request.
fn wait_for_supervisor_release(path: Option<&std::path::Path>) {
    let Some(path) = path else { return };
    if let Ok(file) = File::open(path) {
        let _ = file.lock_shared();
    }
}

/// Supervisor side of the cancellation channel: an exclusive lock held on a
/// private temporary file until stop or supervisor exit.
struct CancelChannel {
    file: Option<tempfile::NamedTempFile>,
}
impl CancelChannel {
    fn create() -> Result<Self> {
        let file = tempfile::Builder::new().prefix("beskid-matrix-cancel-").tempfile()?;
        file.as_file().lock()?;
        Ok(Self { file: Some(file) })
    }
    fn path(&self) -> Option<&std::path::Path> {
        self.file.as_ref().map(|file| file.path())
    }
    fn release(&mut self) {
        // Dropping the handle releases the lock; the file is then removed.
        self.file.take();
    }
}

fn execute_worker(args: TestArgs) -> Result<()> {
    let budgets = args.execution_budgets();
    let cancellation = Cancellation::default();
    let worker_cancellation = cancellation.clone();
    let cancel_lock_path = env::var_os(MATRIX_CANCEL_LOCK_ENV).map(std::path::PathBuf::from);
    thread::spawn(move || {
        // The supervisor holds an exclusive lock on a private file. The lock
        // is released on an explicit stop and by the OS on parent death, which
        // cancels existing native phase controls. Stdin is never used here:
        // the tested program owns a null stdin with deterministic EOF.
        wait_for_supervisor_release(cancel_lock_path.as_deref());
        worker_cancellation.cancel();
    });
    let mut workspace = match PreparedWorkspace::prepare(&args, budgets, cancellation) {
        Ok(workspace) => workspace,
        Err(error) => return emit_fatal("resolve_materialize_salsa_engine", error),
    };
    let all_targets = workspace.test_targets();
    if all_targets.is_empty() {
        return emit_fatal(
            "target_inventory",
            anyhow!("no Test or Lib targets in {}", workspace.manifest_path().display()),
        );
    }
    let (selected_targets, env_filtered) = match filter_targets_by_env(all_targets.clone()) {
        Ok(filtered) => filtered,
        Err(error) => return emit_fatal("target_inventory", error),
    };
    let cli_filtered = !args.include_tags.is_empty() || !args.exclude_tags.is_empty() || args.group.is_some();
    let shard_targets = match worker_shard().map(|(index, count)| shard_of(&selected_targets, index, count)) {
        Ok(targets) => targets,
        Err(error) => return emit_fatal("target_inventory", error),
    };
    if let Err(error) = workspace.activate_mods(&shard_targets, &args) {
        return emit_fatal("activate_mods", error);
    }
    emit_event(&WorkerEvent::Prepared {
        manifest: workspace.manifest_path().to_path_buf(),
        revisions: workspace.revisions(),
        expected_targets: all_targets,
        selected_targets: selected_targets.clone(),
        shard_targets: shard_targets.clone(),
        filtered: env_filtered || cli_filtered,
    })?;
    // A target whose preparation or native build fails is reported as failed with its phase and
    // error; it never stops the other targets of the shard. Workspace mutation fails every target.
    let mut prepared_targets = Vec::with_capacity(shard_targets.len());
    for name in &shard_targets {
        let started = (unix_ms(), Instant::now());
        let prepared = workspace.prepare_targets(std::slice::from_ref(name), |target| {
            emit_event(&WorkerEvent::TargetStarted { target: target.to_string(), phase: "prepare_target".to_string() })
        });
        prepared_targets.push(match prepared {
            Ok(mut targets) => targets.pop().ok_or_else(|| (name.clone(), started, anyhow!("no prepared target"))),
            Err(error) => Err((name.clone(), started, error)),
        });
    }

    let mut failed = false;
    for prepared in prepared_targets {
        let report = match prepared {
            Ok(target) => {
                emit_event(&WorkerEvent::TargetStarted {
                    target: target.name.clone(),
                    phase: "execute_tests".to_string(),
                })?;
                let target_name = target.name.clone();
                let started = (unix_ms(), Instant::now());
                match execute_prepared_target(&mut workspace, target, &args, false, &mut |test| {
                    emit_event(&WorkerEvent::TestStarted { target: target_name.clone(), test: test.to_string() })
                }) {
                    Ok(report) => report,
                    Err(error) => failed_target_report(target_name, "execute_tests", started, &error),
                }
            }
            Err((name, started, error)) => failed_target_report(name, "prepare_target", started, &error),
        };
        failed |= report.result != TargetResult::Passed;
        emit_event(&WorkerEvent::TargetFinished { report })?;
    }
    if failed { Err(anyhow!("one or more matrix targets failed")) } else { Ok(()) }
}

fn failed_target_report(target: String, phase: &str, started: (u64, Instant), error: &anyhow::Error) -> TargetReport {
    TargetReport {
        target,
        started_unix_ms: started.0,
        ended_unix_ms: unix_ms(),
        duration_ms: duration_ms(started.1.elapsed()),
        active_phase: phase.to_string(),
        last_started_test: None,
        result: TargetResult::Failed,
        tests: TestSummary::default(),
        phases: Vec::new(),
        error: Some(format!("{phase} failed: {error:#}")),
    }
}

/// The shard (`index`, `count`) assigned by the supervisor; fails closed on a malformed value.
fn worker_shard() -> Result<(usize, usize)> {
    let raw = env::var(MATRIX_SHARD_ENV).map_err(|_| anyhow!("matrix worker started without {MATRIX_SHARD_ENV}"))?;
    let parsed = raw
        .split_once('/')
        .and_then(|(index, count)| Some((index.parse::<usize>().ok()?, count.parse::<usize>().ok()?)));
    match parsed {
        Some((index, count)) if count > 0 && index < count => Ok((index, count)),
        _ => Err(anyhow!("{MATRIX_SHARD_ENV} must be `<index>/<count>` with index < count, got `{raw}`")),
    }
}

/// Round-robin shard: neighbouring targets of one package area land on different workers.
fn shard_of(selected: &[String], index: usize, count: usize) -> Vec<String> {
    selected.iter().skip(index).step_by(count).cloned().collect()
}

fn worker_count(args: &TestArgs) -> Result<usize> {
    if args.input.is_some() {
        return Ok(1);
    }
    let requested = match env::var(MATRIX_WORKERS_ENV) {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(count) if count > 0 => count,
            _ => return Err(anyhow!("{MATRIX_WORKERS_ENV} must be a positive integer, got `{raw}`")),
        },
        Err(_) => thread::available_parallelism().map_or(1, |cores| (cores.get() / 4).clamp(1, MAX_DEFAULT_WORKERS)),
    };
    // A worker with an empty shard would still prepare the whole workspace.
    let env_targets = env::var("BESKID_CORELIB_TEST_TARGETS")
        .map(|raw| raw.split(',').filter(|part| !part.trim().is_empty()).count())
        .unwrap_or(0);
    let single_target = args.project.target.is_some();
    Ok(match (single_target, env_targets) {
        (true, _) => 1,
        (false, 0) => requested,
        (false, targets) => requested.min(targets),
    })
}

/// Supervisor state of one shard worker.
struct ShardState {
    active: Option<ActiveTarget>,
    exit: Option<std::process::ExitStatus>,
}

/// Progress lines in manifest target order, printed once every earlier target has finished.
struct OrderedProgress {
    next: usize,
}
impl OrderedProgress {
    fn flush(&mut self, selected: &[String], finished: &HashMap<String, TargetReport>) {
        while let Some(report) = selected.get(self.next).and_then(|target| finished.get(target)) {
            print_target_line(report);
            self.next += 1;
        }
    }
}

fn print_target_line(report: &TargetReport) {
    if report.result == TargetResult::Passed {
        if beskid_telemetry::verbosity() == beskid_telemetry::Verbosity::Quiet {
            return;
        }
        eprintln!("Running {}... PASS ({:.1?})", report.target, Duration::from_millis(report.duration_ms));
    } else {
        eprintln!("Running {}... FAIL: {}", report.target, report.error.as_deref().unwrap_or("target failed"));
    }
}

fn supervise_worker(args: TestArgs) -> Result<()> {
    let budgets = args.execution_budgets();
    let matrix_started = Instant::now();
    let count = worker_count(&args)?;
    let (tx, rx) = mpsc::channel::<(usize, Result<WorkerEvent>)>();
    let mut workers = Vec::with_capacity(count);
    for index in 0..count {
        let mut worker = spawn_worker(&args, index, count)?;
        let stdout = worker.stdout.take().ok_or_else(|| anyhow!("matrix worker stdout was not piped"))?;
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let event = line.map_err(anyhow::Error::from).and_then(|line| {
                    let mut program_stdout = std::io::stdout().lock();
                    decode_worker_line(&line, &mut program_stdout)
                });
                match event {
                    Ok(Some(event)) => {
                        if tx.send((index, Ok(event))).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = tx.send((index, Err(error)));
                        break;
                    }
                }
            }
        });
        workers.push(worker);
    }
    drop(tx);

    let mut matrix = MatrixState {
        report: None,
        selected_targets: Vec::new(),
        finished: HashMap::new(),
        shards: (0..count).map(|_| ShardState { active: None, exit: None }).collect(),
        fatal: None,
        progress: OrderedProgress { next: 0 },
    };
    let mut interruption = None;
    loop {
        while let Ok((index, event)) = rx.try_recv() {
            matrix.apply(index, event)?;
        }
        let matrix_expired = matrix_started.elapsed() >= budgets.matrix;
        let target_expired = matrix
            .shards
            .iter()
            .any(|shard| shard.active.as_ref().is_some_and(|active| active.started.elapsed() >= budgets.target));
        if matrix_expired || target_expired {
            for (worker, shard) in workers.iter_mut().zip(matrix.shards.iter_mut()) {
                shard.exit = kill_and_reap(&mut worker.child, &mut worker.cancel).or(shard.exit);
            }
            interruption = Some(if matrix_expired { "whole-matrix deadline expired" } else { "per-target deadline expired" });
            break;
        }
        // A fatal worker error stops the whole run: its shard can no longer complete.
        if matrix.fatal.is_some() {
            for (worker, shard) in workers.iter_mut().zip(matrix.shards.iter_mut()) {
                if shard.exit.is_none() {
                    shard.exit = kill_and_reap(&mut worker.child, &mut worker.cancel);
                }
            }
        }
        for (worker, shard) in workers.iter_mut().zip(matrix.shards.iter_mut()) {
            if shard.exit.is_none() {
                shard.exit = worker.try_wait()?;
            }
        }
        if matrix.shards.iter().all(|shard| shard.exit.is_some()) {
            while let Ok((index, event)) = rx.recv() {
                matrix.apply(index, event)?;
            }
            break;
        }
        thread::sleep(SUPERVISOR_POLL);
    }
    drop(workers);

    let mut report = match interruption {
        Some(_) => matrix.report.take().unwrap_or_else(|| empty_failed_report(&args)),
        None => {
            let fatal_message =
                matrix.fatal.as_deref().unwrap_or("matrix worker emitted no prepared report").to_owned();
            matrix.report.take().ok_or_else(|| anyhow!(fatal_message))?
        }
    };
    report.worker_exit_cause = merged_exit_cause(&matrix.shards);
    let actives = matrix.shards.iter().filter_map(|shard| shard.active.clone()).collect::<Vec<_>>();
    if let Some(reason) = interruption {
        report.timed_out = true;
        assemble_targets(&mut report, &matrix.selected_targets, matrix.finished, &actives, TargetResult::TimedOut, reason);
    } else if let Some(error) = matrix.fatal.take() {
        assemble_targets(&mut report, &matrix.selected_targets, matrix.finished, &actives, TargetResult::Failed, &error);
        report.cancelled = true;
    } else if matrix.finished.len() < matrix.selected_targets.len() {
        assemble_targets(
            &mut report,
            &matrix.selected_targets,
            matrix.finished,
            &actives,
            TargetResult::Failed,
            "matrix worker exited before completing the target inventory",
        );
        report.cancelled = true;
    } else {
        assemble_targets(&mut report, &matrix.selected_targets, matrix.finished, &actives, TargetResult::Failed, "");
    }
    for target in report.targets.iter().skip(matrix.progress.next) {
        print_target_line(target);
    }
    let current = super::prepared_matrix::revision_snapshot(&report.manifest);
    report.finish_eligibility(&current);
    present_report(&args, &report)?;
    let passed = report.targets.iter().filter(|target| target.result == TargetResult::Passed).count();
    if selected_run_passed(&report) {
        Ok(())
    } else if report.timed_out {
        Err(anyhow!("matrix timed out after {passed}/{} passing target(s)", report.selected))
    } else {
        Err(anyhow!("matrix run failed: {passed}/{} target(s) passed", report.selected))
    }
}

/// The first abnormal worker exit in shard order, otherwise the shared clean exit.
fn merged_exit_cause(shards: &[ShardState]) -> Option<WorkerExitCause> {
    let causes = shards.iter().map(|shard| shard.exit.as_ref().map(worker_exit_cause)).collect::<Vec<_>>();
    if causes.iter().any(Option::is_none) {
        return causes.into_iter().find(|cause| cause.is_some_and(|cause| cause != WorkerExitCause::Exited { code: 0 })).flatten();
    }
    causes
        .iter()
        .flatten()
        .copied()
        .find(|cause| *cause != WorkerExitCause::Exited { code: 0 })
        .or(Some(WorkerExitCause::Exited { code: 0 }))
}

struct MatrixState {
    report: Option<MatrixReport>,
    selected_targets: Vec<String>,
    finished: HashMap<String, TargetReport>,
    shards: Vec<ShardState>,
    fatal: Option<String>,
    progress: OrderedProgress,
}

impl MatrixState {
    fn apply(&mut self, index: usize, event: Result<WorkerEvent>) -> Result<()> {
        let event = event?;
        let shard = &mut self.shards[index];
        match event {
            WorkerEvent::Prepared {
                manifest,
                revisions,
                expected_targets,
                selected_targets: selected,
                shard_targets: _,
                filtered,
            } => {
                if let Some(report) = &self.report {
                    // Every worker prepared the same frozen inventory; a difference means the
                    // workspace changed between worker starts, which no report may hide.
                    if report.revisions != revisions
                        || report.expected_targets != expected_targets
                        || self.selected_targets != selected
                    {
                        self.fatal.get_or_insert_with(|| {
                            format!("matrix worker {index} prepared a different revision or target inventory")
                        });
                    }
                    return Ok(());
                }
                self.selected_targets = selected;
                self.report = Some(MatrixReport {
                    manifest,
                    revisions,
                    denominator: expected_targets.len(),
                    expected_targets,
                    selected: self.selected_targets.len(),
                    filtered,
                    retried: false,
                    ignored: 0,
                    skipped: 0,
                    timed_out: false,
                    cancelled: false,
                    worker_exit_cause: None,
                    release_eligible: false,
                    targets: Vec::new(),
                });
            }
            WorkerEvent::TargetStarted { target, phase } => {
                shard.active = Some(ActiveTarget { target, phase, last_started_test: None, started: Instant::now() });
            }
            WorkerEvent::TestStarted { target, test } => {
                if let Some(current) = shard.active.as_mut().filter(|current| current.target == target) {
                    current.last_started_test = Some(test);
                }
            }
            WorkerEvent::TargetFinished { report: target_report } => {
                if let Some(matrix) = self.report.as_mut() {
                    matrix.skipped += target_report.tests.skipped;
                    matrix.timed_out |= target_report.result == TargetResult::TimedOut;
                }
                shard.active = None;
                self.finished.insert(target_report.target.clone(), target_report);
                self.progress.flush(&self.selected_targets, &self.finished);
            }
            WorkerEvent::Fatal { phase, error } => {
                self.fatal.get_or_insert_with(|| format!("worker {index} failed in phase `{phase}`: {error}"));
            }
        }
        Ok(())
    }
}

fn selected_run_passed(report: &MatrixReport) -> bool {
    !report.timed_out
        && !report.cancelled
        && report.worker_exit_cause == Some(WorkerExitCause::Exited { code: 0 })
        && report.targets.len() == report.selected
        && report.targets.iter().all(|target| target.result == TargetResult::Passed)
}

fn spawn_worker(args: &TestArgs, index: usize, count: usize) -> Result<MatrixWorker> {
    let executable = env::current_exe()?;
    let mut command = Command::new(executable);
    command.arg("test");
    if let Some(input) = &args.input {
        command.arg(input);
    }
    if let Some(project) = &args.project.project {
        command.arg("--project").arg(project);
    }
    if let Some(target) = &args.project.target {
        command.arg("--target").arg(target);
    }
    if let Some(member) = &args.project.workspace_member {
        command.arg("--workspace-member").arg(member);
    }
    if args.lockfile.frozen {
        command.arg("--frozen");
    }
    if args.lockfile.locked {
        command.arg("--locked");
    }
    if args.lockfile.offline {
        command.arg("--offline");
    }
    for tag in &args.include_tags {
        command.arg("--include-tag").arg(tag);
    }
    for tag in &args.exclude_tags {
        command.arg("--exclude-tag").arg(tag);
    }
    if let Some(group) = &args.group {
        command.arg("--group").arg(group);
    }
    if let Some(target_timeout) = args.target_timeout {
        command.arg("--target-timeout").arg(target_timeout.to_string());
    }
    if let Some(matrix_timeout) = args.matrix_timeout {
        command.arg("--matrix-timeout").arg(matrix_timeout.to_string());
    }
    command.arg("--all-targets").arg("--plain");
    let cancel = CancelChannel::create()?;
    let lock_path = cancel.path().map(std::path::Path::to_path_buf).ok_or_else(|| anyhow!("cancel channel missing"))?;
    command
        .env(MATRIX_WORKER_ENV, "1")
        .env(MATRIX_SHARD_ENV, format!("{index}/{count}"))
        .env(MATRIX_CANCEL_LOCK_ENV, lock_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map(|child| MatrixWorker { child, cancel })
        .map_err(Into::into)
}

fn emit_event(event: &WorkerEvent) -> Result<()> {
    println!("{WORKER_EVENT_PREFIX}{}", serde_json::to_string(event)?);
    std::io::stdout().flush()?;
    Ok(())
}

fn decode_worker_line<W: Write>(line: &str, program_stdout: &mut W) -> Result<Option<WorkerEvent>> {
    let Some((output, event)) = line.split_once(WORKER_EVENT_PREFIX) else {
        writeln!(program_stdout, "{line}")?;
        return Ok(None);
    };
    program_stdout.write_all(output.as_bytes())?;
    program_stdout.flush()?;
    serde_json::from_str(event).map(Some).map_err(anyhow::Error::from)
}

fn emit_fatal(phase: &str, error: anyhow::Error) -> Result<()> {
    emit_event(&WorkerEvent::Fatal { phase: phase.to_string(), error: error.to_string() })?;
    Err(error)
}

struct MatrixWorker {
    child: Child,
    cancel: CancelChannel,
}
impl std::ops::Deref for MatrixWorker {
    type Target = Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl std::ops::DerefMut for MatrixWorker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
impl Drop for MatrixWorker {
    fn drop(&mut self) {
        let _ = kill_and_reap(&mut self.child, &mut self.cancel);
    }
}
fn kill_and_reap(child: &mut Child, cancel: &mut CancelChannel) -> Option<std::process::ExitStatus> {
    // Releasing the private lock requests cooperative cancellation even if
    // a parent exception bypassed the normal timeout branch.
    cancel.release();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(SUPERVISOR_POLL),
            _ => break,
        }
    }
    // Synchronous CLIF cannot currently be preempted. Its worker is bounded
    // after the grace period; no native tool should survive cooperative stop.
    let _ = child.kill();
    child.wait().ok()
}

fn worker_exit_cause(status: &std::process::ExitStatus) -> WorkerExitCause {
    if let Some(code) = status.code() {
        return WorkerExitCause::Exited { code };
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return WorkerExitCause::Signaled { signal };
        }
    }
    WorkerExitCause::Unknown
}

/// The final target list in selected (manifest) order: each finished report, the interrupted active
/// target of a worker as `active_result`, and every other unfinished target as cancelled.
fn assemble_targets(
    report: &mut MatrixReport,
    selected: &[String],
    mut finished: HashMap<String, TargetReport>,
    actives: &[ActiveTarget],
    active_result: TargetResult,
    reason: &str,
) {
    for target in selected {
        if let Some(done) = finished.remove(target) {
            report.targets.push(done);
            continue;
        }
        let active = actives.iter().find(|active| active.target == *target);
        let phase = active.map_or("cancelled", |active| active.phase.as_str());
        report.targets.push(TargetReport {
            target: target.clone(),
            started_unix_ms: unix_ms(),
            ended_unix_ms: unix_ms(),
            duration_ms: active.map_or(0, |active| duration_ms(active.started.elapsed())),
            active_phase: phase.to_string(),
            last_started_test: active.and_then(|active| active.last_started_test.clone()),
            result: if active.is_some() { active_result } else { TargetResult::Cancelled },
            tests: TestSummary::default(),
            phases: Vec::new(),
            error: Some(if active.is_some() {
                format!("{reason} while `{target}` was in phase `{phase}`")
            } else {
                format!("cancelled after {reason}")
            }),
        });
    }
}

fn empty_failed_report(args: &TestArgs) -> MatrixReport {
    MatrixReport {
        manifest: args.project.project.clone().or_else(|| args.input.clone()).unwrap_or_default(),
        revisions: RevisionSnapshot { root: None, compiler: None, corelib: None },
        denominator: 0,
        expected_targets: Vec::new(),
        selected: 0,
        filtered: false,
        retried: false,
        ignored: 0,
        skipped: 0,
        timed_out: true,
        cancelled: false,
        worker_exit_cause: None,
        release_eligible: false,
        targets: Vec::new(),
    }
}

fn present_report(args: &TestArgs, report: &MatrixReport) -> Result<()> {
    let passed = report.targets.iter().filter(|target| target.result == TargetResult::Passed).count();
    let failed = report.targets.iter().filter(|target| target.result != TargetResult::Passed).count();
    if args.json {
        let complete = passed == report.selected && !report.timed_out && !report.cancelled;
        crate::machine_output::Records::new("test").Emit(
            "result",
            if complete { "success" } else { "failure" },
            serde_json::to_value(report)?,
        )?;
    } else {
        eprintln!("\nmatrix: {passed}/{} passed, {failed}/{} failed", report.selected, report.selected);
        eprintln!("release eligible: {}", report.release_eligible);
    }
    Ok(())
}

fn filter_targets_by_env(targets: Vec<String>) -> Result<(Vec<String>, bool)> {
    let raw = env::var("BESKID_CORELIB_TEST_TARGETS").unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok((targets, false));
    }
    let wanted: HashSet<String> =
        raw.split(',').map(str::trim).filter(|part| !part.is_empty()).map(str::to_owned).collect();
    if wanted.is_empty() {
        return Ok((targets, false));
    }
    let available: HashSet<&str> = targets.iter().map(String::as_str).collect();
    let missing = wanted.iter().filter(|name| !available.contains(name.as_str())).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(anyhow!("BESKID_CORELIB_TEST_TARGETS unknown targets: {}", missing.join(", ")));
    }
    Ok((targets.into_iter().filter(|name| wanted.contains(name)).collect(), true))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{
        ActiveTarget, CancelChannel, ShardState, WORKER_EVENT_PREFIX, WorkerEvent, assemble_targets,
        decode_worker_line, filter_targets_by_env, merged_exit_cause, selected_run_passed, shard_of,
        wait_for_supervisor_release,
    };
    use crate::commands::prepared_matrix::{MatrixReport, RevisionSnapshot, TargetResult, WorkerExitCause};

    #[test]
    fn empty_filter_preserves_manifest_denominator() {
        unsafe { std::env::remove_var("BESKID_CORELIB_TEST_TARGETS") };
        let targets = vec!["One".to_string(), "Two".to_string()];
        let (selected, filtered) = filter_targets_by_env(targets.clone()).expect("filter");
        assert_eq!(selected, targets);
        assert!(!filtered);
    }

    #[test]
    fn filtered_matrix_succeeds_when_every_selected_target_passes() {
        let expected_targets = vec!["One".to_string(), "Two".to_string()];
        let report = MatrixReport {
            manifest: PathBuf::from("Project.bproj"),
            revisions: RevisionSnapshot { root: None, compiler: None, corelib: None },
            denominator: expected_targets.len(),
            expected_targets,
            selected: 1,
            filtered: true,
            retried: false,
            ignored: 0,
            skipped: 0,
            timed_out: false,
            cancelled: false,
            worker_exit_cause: Some(WorkerExitCause::Exited { code: 0 }),
            release_eligible: false,
            targets: vec![crate::commands::prepared_matrix::TargetReport {
                target: "Two".to_string(),
                started_unix_ms: 1,
                ended_unix_ms: 2,
                duration_ms: 1,
                active_phase: "complete".to_string(),
                last_started_test: Some("Two.passes".to_string()),
                result: TargetResult::Passed,
                tests: crate::commands::test::TestSummary { passed: 1, ..Default::default() },
                phases: Vec::new(),
                error: None,
            }],
        };

        assert!(selected_run_passed(&report));
    }

    #[test]
    fn selected_matrix_fails_when_worker_exits_nonzero_after_all_targets_pass() {
        let mut report = passing_selected_report();
        report.worker_exit_cause = Some(WorkerExitCause::Exited { code: 9 });

        assert!(!selected_run_passed(&report));
    }

    #[test]
    fn selected_matrix_fails_when_worker_is_signaled_after_all_targets_pass() {
        let mut report = passing_selected_report();
        report.worker_exit_cause = Some(WorkerExitCause::Signaled { signal: 9 });

        assert!(!selected_run_passed(&report));
    }

    #[test]
    fn selected_matrix_fails_when_run_is_interrupted_after_all_targets_pass() {
        let mut timed_out = passing_selected_report();
        timed_out.timed_out = true;
        let mut cancelled = passing_selected_report();
        cancelled.cancelled = true;

        assert!(!selected_run_passed(&timed_out));
        assert!(!selected_run_passed(&cancelled));
    }

    fn passing_selected_report() -> MatrixReport {
        MatrixReport {
            manifest: PathBuf::from("Project.bproj"),
            revisions: RevisionSnapshot { root: None, compiler: None, corelib: None },
            denominator: 1,
            expected_targets: vec!["One".to_string()],
            selected: 1,
            filtered: false,
            retried: false,
            ignored: 0,
            skipped: 0,
            timed_out: false,
            cancelled: false,
            worker_exit_cause: Some(WorkerExitCause::Exited { code: 0 }),
            release_eligible: false,
            targets: vec![crate::commands::prepared_matrix::TargetReport {
                target: "One".to_string(),
                started_unix_ms: 1,
                ended_unix_ms: 2,
                duration_ms: 1,
                active_phase: "complete".to_string(),
                last_started_test: Some("One.passes".to_string()),
                result: TargetResult::Passed,
                tests: crate::commands::test::TestSummary { passed: 1, ..Default::default() },
                phases: Vec::new(),
                error: None,
            }],
        }
    }

    #[test]
    fn interrupted_active_target_fails_with_last_test_and_unfinished_targets_are_cancelled_in_manifest_order() {
        let selected = vec!["First".to_string(), "Active".to_string(), "Later".to_string(), "Done".to_string()];
        let mut report = MatrixReport {
            manifest: PathBuf::from("Project.bproj"),
            revisions: RevisionSnapshot { root: None, compiler: None, corelib: None },
            denominator: 4,
            expected_targets: selected.clone(),
            selected: 4,
            filtered: false,
            retried: false,
            ignored: 0,
            skipped: 0,
            timed_out: false,
            cancelled: false,
            worker_exit_cause: None,
            release_eligible: false,
            targets: Vec::new(),
        };
        // Shard workers finish out of manifest order.
        let finished = ["Done", "First"]
            .into_iter()
            .map(|name| {
                (
                    name.to_string(),
                    crate::commands::prepared_matrix::TargetReport {
                        target: name.to_string(),
                        started_unix_ms: 1,
                        ended_unix_ms: 2,
                        duration_ms: 1,
                        active_phase: "complete".to_string(),
                        last_started_test: None,
                        result: TargetResult::Passed,
                        tests: crate::commands::test::TestSummary { passed: 1, ..Default::default() },
                        phases: Vec::new(),
                        error: None,
                    },
                )
            })
            .collect();
        assemble_targets(
            &mut report,
            &selected,
            finished,
            &[ActiveTarget {
                target: "Active".to_string(),
                phase: "execute_tests".to_string(),
                last_started_test: Some("Active.crashes".to_string()),
                started: Instant::now(),
            }],
            TargetResult::Failed,
            "worker exited abnormally",
        );
        let order = report.targets.iter().map(|target| target.target.as_str()).collect::<Vec<_>>();
        assert_eq!(order, ["First", "Active", "Later", "Done"], "reports follow manifest order, not finish order");
        assert_eq!(report.targets[0].result, TargetResult::Passed);
        assert_eq!(report.targets[1].result, TargetResult::Failed);
        assert_eq!(report.targets[1].last_started_test.as_deref(), Some("Active.crashes"));
        assert_eq!(report.targets[2].result, TargetResult::Cancelled);
        assert_eq!(report.targets[2].last_started_test, None);
        assert_eq!(report.targets[3].result, TargetResult::Passed);
    }

    #[test]
    fn shards_partition_selected_targets_round_robin() {
        let selected = (0..7).map(|index| format!("T{index}")).collect::<Vec<_>>();
        let shards = (0..3).map(|index| shard_of(&selected, index, 3)).collect::<Vec<_>>();
        assert_eq!(shards[0], ["T0", "T3", "T6"]);
        assert_eq!(shards[1], ["T1", "T4"]);
        assert_eq!(shards[2], ["T2", "T5"]);
        let mut union = shards.concat();
        union.sort();
        assert_eq!(union, selected, "every selected target runs in exactly one shard");
    }

    #[cfg(unix)]
    #[test]
    fn one_abnormal_worker_exit_decides_the_merged_exit_cause() {
        use std::os::unix::process::ExitStatusExt;
        let shard = |code: i32| ShardState { active: None, exit: Some(std::process::ExitStatus::from_raw(code << 8)) };
        assert_eq!(merged_exit_cause(&[shard(0), shard(0)]), Some(WorkerExitCause::Exited { code: 0 }));
        assert_eq!(merged_exit_cause(&[shard(0), shard(3)]), Some(WorkerExitCause::Exited { code: 3 }));
        assert_eq!(merged_exit_cause(&[shard(0), ShardState { active: None, exit: None }]), None);
    }

    #[test]
    fn prefixed_test_started_event_round_trips() {
        let encoded = format!(
            "noise{}{}",
            WORKER_EVENT_PREFIX,
            serde_json::to_string(&WorkerEvent::TestStarted {
                target: "Target".to_string(),
                test: "Target.case".to_string(),
            })
            .expect("serialize")
        );
        let mut output = Vec::new();
        let event = decode_worker_line(&encoded, &mut output).expect("parse").expect("event");
        assert_eq!(output, b"noise");
        assert!(
            matches!(event, WorkerEvent::TestStarted { target, test } if target == "Target" && test == "Target.case")
        );
    }

    #[test]
    fn worker_protocol_preserves_program_stdout_before_framed_event() {
        let line = format!(
            "corelib{WORKER_EVENT_PREFIX}{{\"event\":\"target_started\",\"target\":\"SystemSyscallWriteTests\",\"phase\":\"execute_tests\"}}"
        );
        let mut output = Vec::new();

        let event = decode_worker_line(&line, &mut output).expect("decode worker line").expect("framed event");

        assert_eq!(output, b"corelib");
        assert!(matches!(
            event,
            WorkerEvent::TargetStarted { target, phase }
                if target == "SystemSyscallWriteTests" && phase == "execute_tests"
        ));
    }

    #[test]
    fn worker_protocol_decodes_target_timing_fields() {
        let line = format!(
            "{WORKER_EVENT_PREFIX}{{\"event\":\"target_finished\",\"report\":{{\"target\":\"One\",\"started_unix_ms\":1,\"ended_unix_ms\":3,\"duration_ms\":2,\"active_phase\":\"complete\",\"result\":\"passed\",\"tests\":{{\"passed\":1,\"failed\":0,\"skipped\":0,\"filtered_out\":0}},\"phases\":[],\"error\":null}}}}"
        );
        let mut output = Vec::new();

        let event = decode_worker_line(&line, &mut output).expect("decode target event").expect("framed event");

        assert!(matches!(event, WorkerEvent::TargetFinished { report } if report.duration_ms == 2));
    }

    #[test]
    fn spawn_worker_forwards_target_timeout_to_the_child() {
        let source = include_str!("matrix_test.rs");
        assert!(
            source.contains("if let Some(target_timeout) = args.target_timeout"),
            "spawn_worker must forward --target-timeout to the isolated matrix worker"
        );
        assert!(
            source.contains("command.arg(\"--target-timeout\").arg(target_timeout.to_string())"),
            "spawn_worker must pass the resolved timeout as `--target-timeout <secs>`"
        );
    }

    #[test]
    fn spawn_worker_forwards_matrix_timeout_to_the_child() {
        let source = include_str!("matrix_test.rs");
        assert!(source.contains("if let Some(matrix_timeout) = args.matrix_timeout"));
        assert!(source.contains("command.arg(\"--matrix-timeout\").arg(matrix_timeout.to_string())"));
    }

    #[test]
    fn cancel_lock_blocks_worker_until_supervisor_releases() {
        let mut channel = CancelChannel::create().expect("create channel");
        let path = channel.path().expect("path").to_path_buf();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            wait_for_supervisor_release(Some(&path));
            let _ = tx.send(());
        });
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "worker must stay blocked while locked");
        channel.release();
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok(), "release must cancel the worker");
    }

    #[test]
    fn cancel_wait_fails_closed_when_lock_file_is_missing() {
        wait_for_supervisor_release(Some(std::path::Path::new("/nonexistent/beskid-cancel-lock")));
    }
}
