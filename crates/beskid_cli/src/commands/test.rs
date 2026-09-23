//! `beskid test` — discover `test` items, filter by tags/group, and run them through the prepared-workspace seam.

use anyhow::{Result, anyhow};
use beskid_engine::services::SyntaxTestItem;
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_tools::diagnostics;
use beskid_tools::pipeline::{tui::FileLineLink, tui::TestRowState, tui::TestRunUi};

use beskid_tools::tui::shell::runtime::RuntimeOp;

use super::prepared_matrix::{
    Cancellation, ExecutionBudgets, PhaseRecord, PreparedTarget, PreparedWorkspace, TargetReport, TargetResult,
    duration_ms, unix_ms,
};

#[derive(Args, Debug, Clone)]
pub struct TestArgs {
    /// The input Beskid file to test
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Include only tests with any of these tags
    #[arg(long = "include-tag")]
    pub include_tags: Vec<String>,

    /// Exclude tests with any of these tags
    #[arg(long = "exclude-tag")]
    pub exclude_tags: Vec<String>,

    /// Include only tests whose group starts with this prefix
    #[arg(long)]
    pub group: Option<String>,

    /// Print machine-readable JSON summary
    #[arg(long)]
    pub json: bool,

    /// Disable animated progress and graph output
    #[arg(long)]
    pub plain: bool,

    /// Run every Test target in the project manifest in one process (shared session).
    #[arg(long)]
    pub all_targets: bool,

    /// Per-target execution budget in seconds before a target is reported as timed out
    /// (also `BESKID_TARGET_TIMEOUT_SECS`; the flag wins if both are set)
    #[arg(long, env = "BESKID_TARGET_TIMEOUT_SECS")]
    pub target_timeout: Option<u64>,
}

impl TestArgs {
    /// Resolves the configured execution budgets, applying [`TestArgs::target_timeout`]
    /// (CLI flag or `BESKID_TARGET_TIMEOUT_SECS` env var, flag wins) over the default
    /// target budget. The matrix budget is not currently overridable.
    pub(crate) fn execution_budgets(&self) -> ExecutionBudgets {
        let mut budgets = ExecutionBudgets::default();
        if let Some(seconds) = self.target_timeout {
            budgets.target = Duration::from_secs(seconds);
        }
        budgets
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum TestOutcome {
    Passed,
    Failed,
    Skipped,
    FilteredOut,
    /// The target's execution budget expired before this test could run. Distinct from
    /// `Skipped` (which means the test's own `skip.condition` was true): a timed-out test
    /// would otherwise have run, so it must not be silently folded into "skipped" coverage.
    TimedOut,
}

#[derive(Debug, Clone, Serialize)]
struct TestExecution {
    name: String,
    qualified_name: String,
    outcome: TestOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct TestSummary {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) filtered_out: usize,
    /// Tests never started because the target's execution budget expired first. Reported
    /// separately from `skipped` so budget-expiry coverage loss is never silent.
    #[serde(default)]
    pub(crate) timed_out: usize,
}

/// Run the test harness for the resolved project and print human or `--json` results.
pub fn execute(args: TestArgs) -> Result<()> {
    if args.all_targets {
        return super::matrix_test::execute_all_targets(args);
    }
    execute_single_target(args, None)
}

/// Same as [`execute`] but forwards pipeline progress into a running `beskid hi` shell.
pub fn execute_for_hi(msg_tx: Sender<RuntimeOp>, args: TestArgs) -> Result<()> {
    if args.all_targets {
        anyhow::bail!("`test --all-targets` is not supported from beskid hi yet");
    }
    execute_single_target(args, Some(msg_tx))
}

fn execute_single_target(args: TestArgs, hi_tx: Option<Sender<RuntimeOp>>) -> Result<()> {
    let budgets = args.execution_budgets();
    let mut workspace = PreparedWorkspace::prepare(&args, hi_tx, budgets, Cancellation::default())?;
    let target_name = workspace
        .test_targets()
        .into_iter()
        .find(|name| args.project.target.as_deref().is_none_or(|selected| selected == name))
        .ok_or_else(|| anyhow!("no Test or Lib target selected"))?;
    let target = workspace
        .prepare_targets(std::slice::from_ref(&target_name), |_| Ok(()))?
        .pop()
        .ok_or_else(|| anyhow!("prepared target inventory was empty"))?;
    let report = execute_prepared_target(&mut workspace, target, &args, true, &mut |_| Ok(()))?;
    if report.result == TargetResult::Passed {
        Ok(())
    } else {
        Err(anyhow!(report.error.unwrap_or_else(|| format!("target `{target_name}` failed"))))
    }
}

pub(crate) fn execute_prepared_target(
    workspace: &mut PreparedWorkspace,
    target: PreparedTarget,
    args: &TestArgs,
    emit: bool,
    on_test_started: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<TargetReport> {
    let target_started = Instant::now();
    let started_unix_ms = unix_ms();
    workspace.reject_mutation("execute_target")?;
    workspace.begin_target_execution();
    let mut phases = Vec::new();
    let tests = target.tests;
    let front = target.front;
    let source_name = target.resolved.source_path.display().to_string();
    let include_tags = normalized_tags(&args.include_tags);
    let exclude_tags = normalized_tags(&args.exclude_tags);
    let hi_attached = workspace.session().pipeline().is_hi_attached();
    let mut test_ui = TestRunUi::new(args.plain, None);
    let mut planned = Vec::new();
    for (row_index, test) in tests.iter().enumerate() {
        let initial = if is_filtered_out(test, &include_tags, &exclude_tags, args.group.as_deref()) {
            TestRowState::FilteredOut
        } else if test.skip_condition == Some(true) {
            TestRowState::Skipped
        } else {
            TestRowState::Pending
        };
        test_ui.push_row(
            test.qualified_name.clone(),
            initial,
            Some(FileLineLink {
                path: target.resolved.source_path.clone(),
                line: test.selection_span.line_col_start.0,
                column: test.selection_span.line_col_start.1,
            }),
        );
        planned.push((test, row_index, initial));
    }
    if emit && !args.json {
        test_ui.draw_initial()?;
    }

    let execute_started = Instant::now();
    let execute_unix_ms = unix_ms();
    let mut executions = Vec::new();
    let mut summary = TestSummary::default();
    let mut timeout_error = None;
    let mut budget_expired = false;
    let mut first_failure = None;
    let mut last_started_test = None;
    let mut planned = planned.into_iter();
    while let Some((test, row_index, initial)) = planned.next() {
        if let Err(error) = workspace.check_budget(&target.name, "execute_tests", Some(target_started)) {
            timeout_error = Some(error);
            budget_expired = true;
            record_timed_out_test(&mut test_ui, &mut executions, &mut summary, emit, args.json, row_index, test)?;
            break;
        }
        if !args.plain && workspace.session().pipeline().interrupted() {
            workspace.cancellation().cancel();
            timeout_error = Some(anyhow!("interrupted while target `{}` was in phase `execute_tests`", target.name));
            break;
        }
        if initial == TestRowState::FilteredOut {
            executions.push(TestExecution {
                name: test.name.to_string(),
                qualified_name: test.qualified_name.clone(),
                outcome: TestOutcome::FilteredOut,
                reason: Some("filtered by CLI options".to_string()),
                output: None,
            });
            summary.filtered_out += 1;
            continue;
        }
        if initial == TestRowState::Skipped {
            let reason = test.skip_reason.as_deref().or(Some("skip.condition is true"));
            if emit && !args.json {
                test_ui.finish_row(row_index, TestRowState::Skipped, Duration::ZERO, reason)?;
            }
            executions.push(TestExecution {
                name: test.name.to_string(),
                qualified_name: test.qualified_name.clone(),
                outcome: TestOutcome::Skipped,
                reason: reason.map(str::to_owned),
                output: None,
            });
            summary.skipped += 1;
            continue;
        }
        on_test_started(&test.qualified_name)?;
        last_started_test = Some(test.qualified_name.clone());
        if emit && !args.json {
            test_ui.start_running(row_index)?;
            if !args.plain {
                workspace.session().pipeline().reset_after_test()?;
            }
        }
        let started = Instant::now();
        match workspace.run_entrypoint(&front, &source_name, &target.resolved.source, &test.qualified_name) {
            Ok(output) => {
                let duration = started.elapsed();
                if emit && !args.json {
                    test_ui.finish_row(row_index, TestRowState::Passed, duration, None)?;
                    if !args.plain {
                        workspace.session().pipeline().reset_after_test()?;
                    }
                }
                executions.push(TestExecution {
                    name: test.name.to_string(),
                    qualified_name: test.qualified_name.clone(),
                    outcome: TestOutcome::Passed,
                    reason: None,
                    output: Some(output),
                });
                summary.passed += 1;
                if target_started.elapsed() >= workspace.target_timeout() {
                    timeout_error = Some(anyhow!(
                        "{}-second target budget expired for `{}` in phase `execute_tests`",
                        workspace.target_timeout().as_secs(),
                        target.name
                    ));
                    budget_expired = true;
                    workspace.cancellation().cancel();
                    break;
                }
            }
            Err(error) => {
                let duration = started.elapsed();
                let reason = if args.json {
                    error.to_string()
                } else {
                    diagnostics::format_report(&diagnostics::report_from_anyhow(&error)).to_string()
                };
                if first_failure.is_none() {
                    first_failure = Some(format!("{}: {}", test.qualified_name, reason.trim()));
                }
                if emit {
                    let detail =
                        format!("\n  FAIL {name}: {reason}", name = test.qualified_name, reason = reason.trim());
                    if test_ui.is_plain() {
                        eprintln!("{detail}");
                    } else {
                        log::error!(target: "beskid.tools.test", "{detail}");
                    }
                    test_ui.finish_row(row_index, TestRowState::Failed, duration, Some(&reason))?;
                }
                executions.push(TestExecution {
                    name: test.name.to_string(),
                    qualified_name: test.qualified_name.clone(),
                    outcome: TestOutcome::Failed,
                    reason: Some(reason),
                    output: None,
                });
                summary.failed += 1;
            }
        }
    }
    if budget_expired {
        for (test, row_index, _initial) in planned {
            record_timed_out_test(&mut test_ui, &mut executions, &mut summary, emit, args.json, row_index, test)?;
        }
    }

    // A `compile-fail` project (by convention, an entry path with a `compile-fail` path
    // component — see `corelib/packages/network/tests/compile-fail/` and
    // `corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail/`) exists to prove the
    // compiler rejects its source. If the target's front end and lowering accepted it cleanly
    // and it declares no `test` items, nothing was ever proven: reporting that as `Passed` would
    // let the fixture regress silently (the exact scenario this check exists to close — see
    // `network_rejections.bproj`'s `RawHandle`/`UdpIsNotStream` targets). A `compile-fail`
    // project with `test` items still runs them normally: those targets prove rejection through
    // a test assertion (e.g. `Assert.Fail` on the "unreachable if correctly rejected" path)
    // rather than through a lowering error, so an empty-test vacuous pass is the only case that
    // must fail closed here.
    let compile_fail_target_without_tests = tests.is_empty() && is_compile_fail_source_path(&target.resolved.source_path);

    let result = if timeout_error.is_some() {
        TargetResult::TimedOut
    } else if summary.failed > 0 {
        TargetResult::Failed
    } else if compile_fail_target_without_tests {
        TargetResult::Failed
    } else {
        TargetResult::Passed
    };
    phases.push(phase_record("execute_tests", execute_unix_ms, execute_started, result));
    if emit {
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "target": target.name, "summary": summary, "tests": executions,
                }))?
            );
        } else if tests.is_empty() {
            if compile_fail_target_without_tests {
                eprintln!(
                    "No tests found, but `{}` is a compile-fail target: it must prove rejection through a \
                     lowering error or a `test` item, and did neither.",
                    source_name
                );
            } else {
                println!("No tests found.");
            }
        } else {
            test_ui.print_summary(
                summary.passed,
                summary.failed,
                summary.skipped,
                summary.filtered_out,
                summary.timed_out,
            )?;
            if !args.plain && !hi_attached {
                workspace.session().pipeline().wait_for_dismiss()?;
            }
        }
    }

    let error = timeout_error
        .map(|error| error.to_string())
        .or_else(|| {
            (summary.failed > 0).then(|| {
                format!(
                    "{} test(s) failed; first failure: {}",
                    summary.failed,
                    first_failure.as_deref().unwrap_or("unknown test failure")
                )
            })
        })
        .or_else(|| {
            compile_fail_target_without_tests.then(|| {
                format!(
                    "compile-fail target `{}` compiled with no diagnostics and declared no `test` items; \
                     it must prove rejection one of those two ways",
                    target.name
                )
            })
        });
    if result == TargetResult::Passed {
        workspace.session().pipeline().finish_session("Tests complete");
    } else {
        workspace.session().pipeline().finish_session("Tests failed");
    }
    workspace.reject_mutation("execute_target")?;
    Ok(TargetReport {
        target: target.name,
        started_unix_ms,
        ended_unix_ms: unix_ms(),
        duration_ms: duration_ms(target_started.elapsed()),
        active_phase: "complete".to_string(),
        last_started_test,
        result,
        tests: summary,
        phases,
        error,
    })
}

/// Records a test that never ran because the target's execution budget expired first.
/// Distinct from a `skip.condition`-driven skip: this test would otherwise have executed,
/// so folding it into "skipped" would silently understate lost coverage.
fn record_timed_out_test(
    test_ui: &mut TestRunUi<'_>,
    executions: &mut Vec<TestExecution>,
    summary: &mut TestSummary,
    emit: bool,
    json: bool,
    row_index: usize,
    test: &SyntaxTestItem,
) -> Result<()> {
    if emit && !json {
        test_ui.finish_row(row_index, TestRowState::TimedOut, Duration::ZERO, Some("target execution budget expired"))?;
    }
    executions.push(TestExecution {
        name: test.name.to_string(),
        qualified_name: test.qualified_name.clone(),
        outcome: TestOutcome::TimedOut,
        reason: Some("target execution budget expired before this test could run".to_string()),
        output: None,
    });
    summary.timed_out += 1;
    Ok(())
}

fn phase_record(phase: &str, started_unix_ms: u64, started: Instant, result: TargetResult) -> PhaseRecord {
    PhaseRecord {
        phase: phase.to_string(),
        started_unix_ms,
        ended_unix_ms: unix_ms(),
        duration_ms: duration_ms(started.elapsed()),
        result,
    }
}

/// Whether `path` sits under a `compile-fail` directory, by the convention already established
/// by `corelib/packages/network/tests/compile-fail/` and
/// `corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail/`: a project whose entire
/// purpose is proving the compiler rejects its source, so an empty test run there is a
/// regression, not a pass. Matches the exact path component `compile-fail` case-sensitively, so
/// an ordinary target's own naming (e.g. `compile_fail`, `CompileFail`) is never swept in by
/// accident.
fn is_compile_fail_source_path(source_path: &std::path::Path) -> bool {
    source_path.components().any(|component| component.as_os_str() == "compile-fail")
}

#[cfg(test)]
mod compile_fail_vacuous_pass_tests {
    use super::is_compile_fail_source_path;
    use std::path::Path;

    #[test]
    fn recognizes_the_established_compile_fail_directory_convention() {
        assert!(is_compile_fail_source_path(Path::new(
            "corelib/packages/network/tests/compile-fail/RawHandle.bd"
        )));
        assert!(is_compile_fail_source_path(Path::new(
            "corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail/LegacyCollectionsNamespace.bd"
        )));
    }

    #[test]
    fn does_not_match_an_ordinary_target_outside_a_compile_fail_directory() {
        assert!(!is_compile_fail_source_path(Path::new("corelib/packages/network/src/Network/Tcp/TcpStream.bd")));
        // A near-miss spelling must not accidentally match: this check is deliberately an exact
        // path-component match, not a substring search.
        assert!(!is_compile_fail_source_path(Path::new("src/compile_fail_notes/Notes.bd")));
    }
}

fn normalized_tags(tags: &[String]) -> Vec<String> {
    tags.iter().map(|tag| tag.trim().to_string()).filter(|tag| !tag.is_empty()).collect()
}

fn is_filtered_out(
    test: &SyntaxTestItem,
    include_tags: &[String],
    exclude_tags: &[String],
    group_prefix: Option<&str>,
) -> bool {
    if !include_tags.is_empty() && !test.tags.iter().any(|tag| include_tags.iter().any(|include| include == tag)) {
        return true;
    }
    if test.tags.iter().any(|tag| exclude_tags.iter().any(|exclude| exclude == tag)) {
        return true;
    }
    if let Some(prefix) = group_prefix {
        return test.group.as_ref().is_none_or(|group| !group.starts_with(prefix));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::syntax::common::span::SpanInfo;

    fn sample_project_resolve_args() -> ProjectResolveArgs {
        ProjectResolveArgs { project: None, target: None, workspace_member: None }
    }

    fn sample_lockfile_args() -> LockfilePolicyArgs {
        LockfilePolicyArgs { frozen: false, locked: false }
    }

    fn sample_test(name: &str) -> SyntaxTestItem {
        SyntaxTestItem {
            name: name.to_string(),
            qualified_name: format!("Suite.{name}"),
            tags: Vec::new(),
            group: None,
            skip_condition: None,
            skip_reason: None,
            selection_span: SpanInfo { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) },
        }
    }

    #[test]
    fn timed_out_outcome_serializes_distinctly_from_skipped() {
        let timed_out = serde_json::to_value(&TestOutcome::TimedOut).unwrap();
        let skipped = serde_json::to_value(&TestOutcome::Skipped).unwrap();
        assert_eq!(timed_out, serde_json::json!("timed_out"));
        assert_eq!(skipped, serde_json::json!("skipped"));
        assert_ne!(timed_out, skipped);
    }

    #[test]
    fn test_summary_default_has_zero_timed_out() {
        assert_eq!(TestSummary::default().timed_out, 0);
    }

    #[test]
    fn test_summary_deserializes_without_timed_out_field_present() {
        // Older/other-process JSON (e.g. the matrix worker protocol) predating this field
        // must still deserialize, defaulting the count to zero.
        let summary: TestSummary =
            serde_json::from_str(r#"{"passed":1,"failed":0,"skipped":0,"filtered_out":0}"#).unwrap();
        assert_eq!(summary.timed_out, 0);
        assert_eq!(summary.passed, 1);
    }

    #[test]
    fn record_timed_out_test_reports_timeout_not_skip() {
        let mut test_ui = TestRunUi::new(true, None);
        let mut executions = Vec::new();
        let mut summary = TestSummary::default();
        let test = sample_test("UnexecutedTest");

        record_timed_out_test(&mut test_ui, &mut executions, &mut summary, true, false, 0, &test).unwrap();

        assert_eq!(summary.timed_out, 1);
        assert_eq!(summary.skipped, 0);
        assert_eq!(executions.len(), 1);
        assert!(matches!(executions[0].outcome, TestOutcome::TimedOut));
        assert_eq!(executions[0].qualified_name, "Suite.UnexecutedTest");
        assert!(executions[0].reason.as_deref().unwrap().contains("budget expired"));
    }

    #[test]
    fn execution_budgets_defaults_to_120_seconds_target_timeout() {
        let args = TestArgs {
            input: None,
            project: sample_project_resolve_args(),
            lockfile: sample_lockfile_args(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            group: None,
            json: false,
            plain: true,
            all_targets: false,
            target_timeout: None,
        };
        assert_eq!(args.execution_budgets().target, Duration::from_secs(120));
    }

    #[test]
    fn execution_budgets_honors_explicit_target_timeout() {
        let args = TestArgs {
            input: None,
            project: sample_project_resolve_args(),
            lockfile: sample_lockfile_args(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            group: None,
            json: false,
            plain: true,
            all_targets: false,
            target_timeout: Some(5),
        };
        assert_eq!(args.execution_budgets().target, Duration::from_secs(5));
    }
}
