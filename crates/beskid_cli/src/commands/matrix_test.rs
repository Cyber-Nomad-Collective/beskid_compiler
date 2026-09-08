//! Corelib matrix presentation over one isolated prepared-workspace worker.

use std::collections::HashSet;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use super::prepared_matrix::{
    Cancellation, ExecutionBudgets, MatrixReport, PreparedWorkspace, RevisionSnapshot, TargetReport, TargetResult,
    WorkerExitCause, duration_ms, unix_ms,
};
use super::test::{TestArgs, TestSummary, execute_prepared_target};

const MATRIX_WORKER_ENV: &str = "BESKID_PREPARED_MATRIX_WORKER";
const MATRIX_EVENT_PREFIX: &str = "\u{1e}BESKID_MATRIX_EVENT ";
const SUPERVISOR_POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum WorkerEvent {
    Prepared {
        manifest: std::path::PathBuf,
        revisions: RevisionSnapshot,
        expected_targets: Vec<String>,
        selected_targets: Vec<String>,
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

fn execute_worker(args: TestArgs) -> Result<()> {
    let budgets = ExecutionBudgets::default();
    let cancellation = Cancellation::default();
    let mut workspace = match PreparedWorkspace::prepare(&args, None, budgets, cancellation) {
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
    emit_event(&WorkerEvent::Prepared {
        manifest: workspace.manifest_path().to_path_buf(),
        revisions: workspace.revisions(),
        expected_targets: all_targets,
        selected_targets: selected_targets.clone(),
        filtered: env_filtered || cli_filtered,
    })?;
    let prepared_targets = match workspace.prepare_targets(&selected_targets, |target| {
        emit_event(&WorkerEvent::TargetStarted { target: target.to_string(), phase: "prepare_target".to_string() })
    }) {
        Ok(targets) => targets,
        Err(error) => return emit_fatal("prepare_targets", error),
    };

    let mut failed = false;
    for target in prepared_targets {
        emit_event(&WorkerEvent::TargetStarted { target: target.name.clone(), phase: "execute_tests".to_string() })?;
        let target_name = target.name.clone();
        let report = match execute_prepared_target(&mut workspace, target, &args, false, &mut |test| {
            emit_event(&WorkerEvent::TestStarted { target: target_name.clone(), test: test.to_string() })
        }) {
            Ok(report) => report,
            Err(error) => return emit_fatal("execute_target", error),
        };
        failed |= report.result != TargetResult::Passed;
        emit_event(&WorkerEvent::TargetFinished { report })?;
    }
    if failed { Err(anyhow!("one or more matrix targets failed")) } else { Ok(()) }
}

fn supervise_worker(args: TestArgs) -> Result<()> {
    let budgets = ExecutionBudgets::default();
    let matrix_started = Instant::now();
    let mut child = spawn_worker(&args)?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("matrix worker stdout was not piped"))?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line.map_err(anyhow::Error::from).and_then(|line| parse_worker_event_line(&line)) {
                Ok(Some(event)) => {
                    if tx.send(Ok(event)).is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = tx.send(Err(error));
                    break;
                }
            }
        }
    });

    let mut report: Option<MatrixReport> = None;
    let mut selected_targets = Vec::new();
    let mut active: Option<ActiveTarget> = None;
    let mut fatal = None;
    loop {
        while let Ok(event) = rx.try_recv() {
            match event? {
                WorkerEvent::Prepared {
                    manifest,
                    revisions,
                    expected_targets,
                    selected_targets: selected,
                    filtered,
                } => {
                    selected_targets = selected;
                    report = Some(MatrixReport {
                        manifest,
                        revisions,
                        denominator: expected_targets.len(),
                        expected_targets,
                        selected: selected_targets.len(),
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
                    eprint!("Running {target}... ");
                    let _ = std::io::stderr().flush();
                    active = Some(ActiveTarget { target, phase, last_started_test: None, started: Instant::now() });
                }
                WorkerEvent::TestStarted { target, test } => {
                    if let Some(current) = active.as_mut().filter(|current| current.target == target) {
                        current.last_started_test = Some(test);
                    }
                }
                WorkerEvent::TargetFinished { report: target_report } => {
                    if target_report.result == TargetResult::Passed {
                        eprintln!("PASS ({:.1?})", Duration::from_millis(target_report.duration_ms));
                    } else {
                        eprintln!("FAIL: {}", target_report.error.as_deref().unwrap_or("target failed"));
                    }
                    if let Some(matrix) = report.as_mut() {
                        matrix.skipped += target_report.tests.skipped;
                        matrix.timed_out |= target_report.result == TargetResult::TimedOut;
                        matrix.targets.push(target_report);
                    }
                    active = None;
                }
                WorkerEvent::Fatal { phase, error } => {
                    fatal = Some(format!("worker failed in phase `{phase}`: {error}"))
                }
            }
        }

        let matrix_expired = matrix_started.elapsed() >= budgets.matrix;
        let target_expired = active.as_ref().is_some_and(|active| active.started.elapsed() >= budgets.target);
        if matrix_expired || target_expired {
            let exit = kill_and_reap(&mut child);
            let matrix = report.get_or_insert_with(|| empty_failed_report(&args));
            matrix.worker_exit_cause = exit.as_ref().map(worker_exit_cause);
            matrix.timed_out = true;
            append_interrupted_targets(
                matrix,
                &selected_targets,
                active.take(),
                TargetResult::TimedOut,
                if matrix_expired { "whole-matrix deadline expired" } else { "per-target deadline expired" },
            );
            break;
        }
        if let Some(status) = child.try_wait()? {
            drain_events_until_closed(&rx, &mut report, &mut selected_targets, &mut active, &mut fatal)?;
            if let Some(matrix) = report.as_mut() {
                matrix.worker_exit_cause = Some(worker_exit_cause(&status));
            }
            break;
        }
        thread::sleep(SUPERVISOR_POLL);
    }

    let fatal = fatal;
    let fatal_message = fatal.as_deref().unwrap_or("matrix worker emitted no prepared report").to_owned();
    let mut report = report.ok_or_else(|| anyhow!(fatal_message))?;
    if let Some(error) = fatal {
        append_interrupted_targets(&mut report, &selected_targets, active, TargetResult::Failed, &error);
        report.cancelled = true;
    } else if report.targets.len() < selected_targets.len() && !report.timed_out {
        append_interrupted_targets(
            &mut report,
            &selected_targets,
            active,
            TargetResult::Failed,
            "matrix worker exited before completing the target inventory",
        );
        report.cancelled = true;
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

fn selected_run_passed(report: &MatrixReport) -> bool {
    !report.timed_out
        && !report.cancelled
        && report.worker_exit_cause == Some(WorkerExitCause::Exited { code: 0 })
        && report.targets.len() == report.selected
        && report.targets.iter().all(|target| target.result == TargetResult::Passed)
}

fn spawn_worker(args: &TestArgs) -> Result<Child> {
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
    for tag in &args.include_tags {
        command.arg("--include-tag").arg(tag);
    }
    for tag in &args.exclude_tags {
        command.arg("--exclude-tag").arg(tag);
    }
    if let Some(group) = &args.group {
        command.arg("--group").arg(group);
    }
    command.arg("--all-targets").arg("--plain");
    command
        .env(MATRIX_WORKER_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(Into::into)
}

fn emit_event(event: &WorkerEvent) -> Result<()> {
    println!("{MATRIX_EVENT_PREFIX}{}", serde_json::to_string(event)?);
    std::io::stdout().flush()?;
    Ok(())
}

fn parse_worker_event_line(line: &str) -> Result<Option<WorkerEvent>> {
    let Some((_, payload)) = line.split_once(MATRIX_EVENT_PREFIX) else {
        return Ok(None);
    };
    serde_json::from_str(payload).map(Some).map_err(Into::into)
}

fn emit_fatal(phase: &str, error: anyhow::Error) -> Result<()> {
    emit_event(&WorkerEvent::Fatal { phase: phase.to_string(), error: error.to_string() })?;
    Err(error)
}

fn kill_and_reap(child: &mut Child) -> Option<std::process::ExitStatus> {
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

fn append_interrupted_targets(
    report: &mut MatrixReport,
    selected: &[String],
    active: Option<ActiveTarget>,
    active_result: TargetResult,
    reason: &str,
) {
    let completed = report.targets.len();
    for target in selected.iter().skip(completed) {
        let is_active = active.as_ref().is_some_and(|active| active.target == *target);
        let phase = if is_active {
            active.as_ref().map(|active| active.phase.as_str()).unwrap_or("worker")
        } else {
            "cancelled"
        };
        report.targets.push(TargetReport {
            target: target.clone(),
            started_unix_ms: unix_ms(),
            ended_unix_ms: unix_ms(),
            duration_ms: if is_active {
                active.as_ref().map(|active| duration_ms(active.started.elapsed())).unwrap_or(0)
            } else {
                0
            },
            active_phase: phase.to_string(),
            last_started_test: active
                .as_ref()
                .and_then(|active| active.last_started_test.clone())
                .filter(|_| is_active),
            result: if is_active { active_result } else { TargetResult::Cancelled },
            tests: TestSummary::default(),
            phases: Vec::new(),
            error: Some(if is_active {
                format!("{reason} while `{target}` was in phase `{phase}`")
            } else {
                format!("cancelled after {reason}")
            }),
        });
    }
}

fn drain_events_until_closed(
    rx: &Receiver<Result<WorkerEvent>>,
    report: &mut Option<MatrixReport>,
    selected_targets: &mut Vec<String>,
    active: &mut Option<ActiveTarget>,
    fatal: &mut Option<String>,
) -> Result<()> {
    loop {
        match rx.recv() {
            Ok(event) => apply_worker_event(event?, report, selected_targets, active, fatal),
            Err(_) => return Ok(()),
        }
    }
}

fn apply_worker_event(
    event: WorkerEvent,
    report: &mut Option<MatrixReport>,
    selected_targets: &mut Vec<String>,
    active: &mut Option<ActiveTarget>,
    fatal: &mut Option<String>,
) {
    match event {
        WorkerEvent::Prepared { manifest, revisions, expected_targets, selected_targets: selected, filtered } => {
            *selected_targets = selected;
            *report = Some(MatrixReport {
                manifest,
                revisions,
                denominator: expected_targets.len(),
                expected_targets,
                selected: selected_targets.len(),
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
            *active = Some(ActiveTarget { target, phase, last_started_test: None, started: Instant::now() });
        }
        WorkerEvent::TestStarted { target, test } => {
            if let Some(current) = active.as_mut().filter(|current| current.target == target) {
                current.last_started_test = Some(test);
            }
        }
        WorkerEvent::TargetFinished { report: target_report } => {
            if let Some(matrix) = report.as_mut() {
                matrix.skipped += target_report.tests.skipped;
                matrix.timed_out |= target_report.result == TargetResult::TimedOut;
                matrix.targets.push(target_report);
            }
            *active = None;
        }
        WorkerEvent::Fatal { phase, error } => *fatal = Some(format!("worker failed in phase `{phase}`: {error}")),
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
        println!("{}", serde_json::to_string_pretty(report)?);
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
    use std::time::Instant;

    use super::{
        ActiveTarget, WorkerEvent, append_interrupted_targets, filter_targets_by_env, parse_worker_event_line,
        selected_run_passed,
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
    fn interrupted_active_target_fails_with_last_test_and_downstream_is_cancelled() {
        let selected = vec!["Active".to_string(), "Later".to_string()];
        let mut report = MatrixReport {
            manifest: PathBuf::from("Project.bproj"),
            revisions: RevisionSnapshot { root: None, compiler: None, corelib: None },
            denominator: 2,
            expected_targets: selected.clone(),
            selected: 2,
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
        append_interrupted_targets(
            &mut report,
            &selected,
            Some(ActiveTarget {
                target: "Active".to_string(),
                phase: "execute_tests".to_string(),
                last_started_test: Some("Active.crashes".to_string()),
                started: Instant::now(),
            }),
            TargetResult::Failed,
            "worker exited abnormally",
        );
        assert_eq!(report.targets[0].result, TargetResult::Failed);
        assert_eq!(report.targets[0].last_started_test.as_deref(), Some("Active.crashes"));
        assert_eq!(report.targets[1].result, TargetResult::Cancelled);
        assert_eq!(report.targets[1].last_started_test, None);
    }

    #[test]
    fn prefixed_test_started_event_round_trips() {
        let encoded = format!(
            "noise{}{}",
            super::MATRIX_EVENT_PREFIX,
            serde_json::to_string(&WorkerEvent::TestStarted {
                target: "Target".to_string(),
                test: "Target.case".to_string(),
            })
            .expect("serialize")
        );
        let event = parse_worker_event_line(&encoded).expect("parse").expect("event");
        assert!(
            matches!(event, WorkerEvent::TestStarted { target, test } if target == "Target" && test == "Target.case")
        );
    }
}
