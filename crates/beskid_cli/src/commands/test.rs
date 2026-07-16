//! `beskid test` — discover `test` items, filter by tags/group, and run them under JIT.

use anyhow::{Result, anyhow};
use beskid_engine::Engine;
use beskid_engine::services::{
    SyntaxTestItem, run_entrypoint_from_front_end_with_engine, syntax_test_items_from_front_end,
};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_tools::PipelineProgressKind;
use beskid_tools::diagnostics;
use beskid_tools::pipeline::{tui::FileLineLink, tui::TestRowState, tui::TestRunUi};
use beskid_tools::session::{CommandSession, ResolveInputArgs, SemanticGateOptions};
use beskid_tools::tui::shell::runtime::RuntimeOp;

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

#[derive(Debug, Default, Clone, Serialize)]
struct TestSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    filtered_out: usize,
}

/// Run the test harness for the resolved project and print human or `--json` results.
pub fn execute(args: TestArgs) -> Result<()> {
    if args.all_targets {
        return super::matrix_test::execute_all_targets(args);
    }
    execute_single_target(args, None, None)
}

/// Same as [`execute`] but forwards pipeline progress into a running `beskid hi` shell.
pub fn execute_for_hi(msg_tx: Sender<RuntimeOp>, args: TestArgs) -> Result<()> {
    if args.all_targets {
        anyhow::bail!("`test --all-targets` is not supported from beskid hi yet");
    }
    execute_single_target(args, None, Some(msg_tx))
}

pub(crate) fn execute_single_target(
    args: TestArgs,
    shared_engine: Option<&mut Engine>,
    hi_tx: Option<Sender<RuntimeOp>>,
) -> Result<()> {
    let resolve_args = ResolveInputArgs {
        input: args.input.as_ref(),
        project: args.project.project.as_ref(),
        target: args.project.target.as_deref(),
        workspace_member: args.project.workspace_member.as_deref(),
        frozen: args.lockfile.frozen,
        locked: args.lockfile.locked,
    };
    let (session, resolved) = match hi_tx {
        None => CommandSession::open_and_resolve(
            args.plain,
            PipelineProgressKind::PrepareAndRun,
            &resolve_args,
        )?,
        Some(tx) => {
            let session =
                CommandSession::with_attached_pipeline(tx, PipelineProgressKind::PrepareAndRun);
            let resolved = session.resolve_input(&resolve_args)?;
            (session, resolved)
        }
    };
    let hi_attached = session.pipeline().is_hi_attached();
    let prepared = session.executable_gate_prepared(
        &resolved,
        SemanticGateOptions {
            finish_prepare_ui: false,
            prepare_message: "Analysis complete",
        },
    )?;

    let tests = syntax_test_items_from_front_end(prepared.executable()?)?;
    if tests.is_empty() {
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "summary": TestSummary::default(),
                    "tests": Vec::<TestExecution>::new(),
                }))?
            );
        } else {
            println!("No tests found.");
        }
        return Ok(());
    }

    let front = prepared.into_executable()?;

    let source_name = resolved.source_path.display().to_string();

    let include_tags: Vec<String> = args
        .include_tags
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();
    let exclude_tags: Vec<String> = args
        .exclude_tags
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();

    let mut test_ui = TestRunUi::new(args.plain, Some(session.pipeline()));
    let mut planned = Vec::new();
    for (row_index, test) in tests.iter().enumerate() {
        let initial = if is_filtered_out(test, &include_tags, &exclude_tags, args.group.as_deref())
        {
            TestRowState::FilteredOut
        } else if test.skip_condition == Some(true) {
            TestRowState::Skipped
        } else {
            TestRowState::Pending
        };
        let link = FileLineLink {
            path: resolved.source_path.clone(),
            line: test.selection_span.line_col_start.0,
            column: test.selection_span.line_col_start.1,
        };
        test_ui.push_row(test.qualified_name.clone(), initial, Some(link));
        planned.push((test, row_index, initial));
    }

    if !args.json {
        test_ui.draw_initial()?;
    }

    let mut executions = Vec::new();
    let mut summary = TestSummary::default();
    let mut owned_engine = Engine::new();
    let engine = shared_engine.unwrap_or(&mut owned_engine);
    for (test, row_index, initial) in planned {
        if !args.plain && session.pipeline().interrupted() {
            return Err(anyhow!("interrupted"));
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
            let reason = test
                .skip_reason
                .as_deref()
                .or(Some("skip.condition is true"));
            if !args.json {
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

        if !args.json {
            test_ui.start_running(row_index)?;
            if !args.plain {
                session.pipeline().reset_after_test()?;
            }
        }
        let started = Instant::now();
        match run_entrypoint_from_front_end_with_engine(
            engine,
            &front,
            &source_name,
            &resolved.source,
            &test.qualified_name,
            Some(session.observer()),
        ) {
            Ok(output) => {
                let duration = started.elapsed();
                if !args.json {
                    test_ui.finish_row(row_index, TestRowState::Passed, duration, None)?;
                    if !args.plain {
                        session.pipeline().reset_after_test()?;
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
                // Always print failure details so users see what went wrong
                let detail = format!(
                    "\n  FAIL {name}: {reason}",
                    name = test.qualified_name,
                    reason = reason.trim()
                );
                if test_ui.is_plain() {
                    eprintln!("{detail}");
                } else {
                    log::error!(target: "beskid.tools.test", "{detail}");
                }
                test_ui.finish_row(row_index, TestRowState::Failed, duration, Some(&reason))?;
                if !args.plain {
                    session.pipeline().reset_after_test()?;
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

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "summary": summary,
                "tests": executions,
            }))?
        );
    } else {
        test_ui.print_summary(
            summary.passed,
            summary.failed,
            summary.skipped,
            summary.filtered_out,
        )?;
        if !args.plain && !hi_attached {
            session.pipeline().wait_for_dismiss()?;
        }
    }

    if summary.failed > 0 {
        session.pipeline().finish_session("Tests failed");
        return Err(anyhow!("{} test(s) failed", summary.failed));
    }
    session.pipeline().finish_session("Tests complete");
    Ok(())
}

fn is_filtered_out(
    test: &SyntaxTestItem,
    include_tags: &[String],
    exclude_tags: &[String],
    group_prefix: Option<&str>,
) -> bool {
    if !include_tags.is_empty() {
        let has_included = test
            .tags
            .iter()
            .any(|tag| include_tags.iter().any(|include| include == tag));
        if !has_included {
            return true;
        }
    }

    if test
        .tags
        .iter()
        .any(|tag| exclude_tags.iter().any(|exclude| exclude == tag))
    {
        return true;
    }

    if let Some(prefix) = group_prefix {
        if let Some(group) = &test.group {
            if !group.starts_with(prefix) {
                return true;
            }
        } else {
            return true;
        }
    }

    false
}
