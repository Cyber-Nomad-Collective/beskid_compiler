//! `beskid test` — discover `test` items, filter by tags/group, and run them through the prepared-workspace seam.

use anyhow::{anyhow, Result};
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
    unix_ms, Cancellation, ExecutionBudgets, PhaseRecord, PreparedTarget, PreparedWorkspace, TargetReport, TargetResult,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum TestOutcome {
    Passed,
    Failed,
    Skipped,
    FilteredOut,
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
    let mut workspace = PreparedWorkspace::prepare(&args, hi_tx, ExecutionBudgets::default(), Cancellation::default())?;
    let target_name = workspace
        .test_targets()
        .into_iter()
        .find(|name| args.project.target.as_deref().is_none_or(|selected| selected == name))
        .ok_or_else(|| anyhow!("no Test or Lib target selected"))?;
    let target = workspace
        .prepare_targets(std::slice::from_ref(&target_name), |_| Ok(()))?
        .pop()
        .ok_or_else(|| anyhow!("prepared target inventory was empty"))?;
    let report = execute_prepared_target(&mut workspace, target, &args, true)?;
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
) -> Result<TargetReport> {
    let target_started = Instant::now();
    let started_unix_ms = unix_ms();
    workspace.reject_mutation("execute_target")?;
    let mut phases = Vec::new();
    let tests = target.tests;
    let front = target.front;
    let source_name = target.resolved.source_path.display().to_string();
    let include_tags = normalized_tags(&args.include_tags);
    let exclude_tags = normalized_tags(&args.exclude_tags);
    let hi_attached = workspace.session().pipeline().is_hi_attached();
    let mut test_ui = TestRunUi::new(args.plain, Some(workspace.session().pipeline()));
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
    for (test, row_index, initial) in planned {
        if let Err(error) = workspace.check_budget(&target.name, "execute_tests", Some(target_started)) {
            timeout_error = Some(error);
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
                if target_started.elapsed() >= workspace.target_timeout() {
                    timeout_error = Some(anyhow!(
                        "120-second target budget expired for `{}` in phase `execute_tests`",
                        target.name
                    ));
                    workspace.cancellation().cancel();
                    break;
                }
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
            }
            Err(error) => {
                let duration = started.elapsed();
                let reason = if args.json {
                    error.to_string()
                } else {
                    diagnostics::format_report(&diagnostics::report_from_anyhow(&error)).to_string()
                };
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

    let result = if timeout_error.is_some() {
        TargetResult::TimedOut
    } else if summary.failed > 0 {
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
            println!("No tests found.");
        } else {
            test_ui.print_summary(summary.passed, summary.failed, summary.skipped, summary.filtered_out)?;
            if !args.plain && !hi_attached {
                workspace.session().pipeline().wait_for_dismiss()?;
            }
        }
    }

    let error = timeout_error
        .map(|error| error.to_string())
        .or_else(|| (summary.failed > 0).then(|| format!("{} test(s) failed", summary.failed)));
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
        duration_ms: target_started.elapsed().as_millis(),
        active_phase: "complete".to_string(),
        result,
        tests: summary,
        phases,
        error,
    })
}

fn phase_record(phase: &str, started_unix_ms: u128, started: Instant, result: TargetResult) -> PhaseRecord {
    PhaseRecord {
        phase: phase.to_string(),
        started_unix_ms,
        ended_unix_ms: unix_ms(),
        duration_ms: started.elapsed().as_millis(),
        result,
    }
}

fn failed_report(
    target: String,
    started_unix_ms: u128,
    started: Instant,
    phase: &str,
    phases: Vec<PhaseRecord>,
    error: anyhow::Error,
) -> TargetReport {
    TargetReport {
        target,
        started_unix_ms,
        ended_unix_ms: unix_ms(),
        duration_ms: started.elapsed().as_millis(),
        active_phase: phase.to_string(),
        result: TargetResult::Failed,
        tests: TestSummary::default(),
        phases,
        error: Some(error.to_string()),
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
