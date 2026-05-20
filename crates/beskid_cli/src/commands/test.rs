//! `beskid test` — discover `test` items, filter by tags/group, and run them under JIT.

use anyhow::{Result, anyhow};
use beskid_analysis::services;
use beskid_engine::services::run_entrypoint_with_pipeline;
use clap::Args;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::errors;
use crate::frontend;
use crate::pipeline_ui::{
    PipelineProgressKind, resolve_input_with_cli_pipeline_kind, tui::FileLineLink,
    tui::TestRowState, tui::TestRunUi, use_cli_spinner,
};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

#[derive(Args, Debug)]
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
    let (pipeline_ui, resolved) = resolve_input_with_cli_pipeline_kind(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        args.plain,
        PipelineProgressKind::PrepareAndRun,
    )?;
    pipeline_ui.show_build_graph(&resolved);
    pipeline_ui.halt_progress_bars_for_output();
    frontend::run_semantic_analysis_gate(
        &resolved.source_path,
        &resolved.source,
        None,
        pipeline_ui.as_ref(),
    )?;
    pipeline_ui.finish_prepare_ui("Analysis complete");
    let program = services::parse_program_with_source_name(
        &resolved.source_path.display().to_string(),
        &resolved.source,
    )?;
    let tests = services::collect_test_cases(&program);
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

    let mut test_ui = TestRunUi::new(args.plain, use_cli_spinner(args.plain));
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
            line: test.definition_line,
            column: test.definition_column,
        };
        test_ui.push_row(test.qualified_name.clone(), initial, Some(link));
        planned.push((test, row_index, initial));
    }

    if !args.json {
        test_ui.draw_initial()?;
    }

    let mut executions = Vec::new();
    let mut summary = TestSummary::default();
    for (test, row_index, initial) in planned {
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
        }
        let started = Instant::now();
        match run_entrypoint_with_pipeline(
            &resolved.source_path,
            &resolved.source,
            &test.name,
            None,
        ) {
            Ok(output) => {
                let duration = started.elapsed();
                if !args.json {
                    test_ui.finish_row(row_index, TestRowState::Passed, duration, None)?;
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
                    errors::format_report(&errors::report_from_anyhow(&error)).to_string()
                };
                if !args.json {
                    if !test_ui.is_plain() {
                        eprint!("{reason}");
                        let _ = std::io::stderr().flush();
                    }
                    test_ui.finish_row(row_index, TestRowState::Failed, duration, None)?;
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
    }

    if summary.failed > 0 {
        pipeline_ui.finish_session("Tests failed");
        return Err(anyhow!("{} test(s) failed", summary.failed));
    }
    pipeline_ui.finish_session("Tests complete");
    Ok(())
}

fn is_filtered_out(
    test: &services::TestCaseInfo,
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
