use anyhow::{Result, anyhow};
use beskid_analysis::services;
use beskid_engine::services::run_entrypoint;
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct TestArgs {
    /// The input Beskid file to test
    pub input: Option<PathBuf>,

    /// Path to a project directory or Project.proj file
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Target name from Project.proj
    #[arg(long)]
    pub target: Option<String>,

    /// Workspace member name when resolving from Workspace.proj
    #[arg(long = "workspace-member")]
    pub workspace_member: Option<String>,

    /// Require lockfile to be up to date and forbid lockfile updates
    #[arg(long)]
    pub frozen: bool,

    /// Require lockfile to exist and match resolution
    #[arg(long)]
    pub locked: bool,

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

pub fn execute(args: TestArgs) -> Result<()> {
    let resolved = services::resolve_input(
        args.input.as_ref(),
        args.project.as_ref(),
        args.target.as_deref(),
        args.workspace_member.as_deref(),
        args.frozen,
        args.locked,
    )?;
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

    let mut executions = Vec::new();
    let mut summary = TestSummary::default();
    for test in tests {
        if is_filtered_out(&test, &include_tags, &exclude_tags, args.group.as_deref()) {
            executions.push(TestExecution {
                name: test.name.clone(),
                qualified_name: test.qualified_name.clone(),
                outcome: TestOutcome::FilteredOut,
                reason: Some("filtered by CLI options".to_string()),
                output: None,
            });
            summary.filtered_out += 1;
            continue;
        }

        if test.skip_condition == Some(true) {
            executions.push(TestExecution {
                name: test.name.clone(),
                qualified_name: test.qualified_name.clone(),
                outcome: TestOutcome::Skipped,
                reason: test
                    .skip_reason
                    .clone()
                    .or_else(|| Some("skip.condition is true".to_string())),
                output: None,
            });
            summary.skipped += 1;
            continue;
        }

        match run_entrypoint(&resolved.source_path, &resolved.source, &test.name) {
            Ok(output) => {
                executions.push(TestExecution {
                    name: test.name.clone(),
                    qualified_name: test.qualified_name.clone(),
                    outcome: TestOutcome::Passed,
                    reason: None,
                    output: Some(output),
                });
                summary.passed += 1;
            }
            Err(error) => {
                executions.push(TestExecution {
                    name: test.name.clone(),
                    qualified_name: test.qualified_name.clone(),
                    outcome: TestOutcome::Failed,
                    reason: Some(error.to_string()),
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
        for execution in &executions {
            match execution.outcome {
                TestOutcome::Passed => println!("PASS {}", execution.qualified_name),
                TestOutcome::Failed => println!(
                    "FAIL {}{}",
                    execution.qualified_name,
                    execution
                        .reason
                        .as_deref()
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                ),
                TestOutcome::Skipped => println!(
                    "SKIP {}{}",
                    execution.qualified_name,
                    execution
                        .reason
                        .as_deref()
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                ),
                TestOutcome::FilteredOut => println!("FILT {}", execution.qualified_name),
            }
        }
        println!(
            "Result: passed={}, failed={}, skipped={}, filtered_out={}",
            summary.passed, summary.failed, summary.skipped, summary.filtered_out
        );
    }

    if summary.failed > 0 {
        return Err(anyhow!("{} test(s) failed", summary.failed));
    }
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
