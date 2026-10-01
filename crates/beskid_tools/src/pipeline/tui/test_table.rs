//! Live test-run table for `beskid test` (status, duration, name).

use std::io;
use std::time::Duration;

use super::hyperlink::{FileLineLink, maybe_link_label};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRowState {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
    FilteredOut,
    /// The target's execution budget expired before this test could run. Distinct from
    /// `Skipped` (an explicit `skip.condition`): coverage was lost, not intentionally skipped.
    TimedOut,
}

#[derive(Debug, Clone)]
pub struct TestRow {
    pub qualified_name: String,
    pub link: Option<FileLineLink>,
    pub state: TestRowState,
    pub duration: Option<Duration>,
    /// Diagnostic text or miette report for failed tests (shown in the code viewer).
    pub failure_detail: Option<String>,
}

/// Line-oriented presenter for a test run.
pub struct TestRunUi {
    rows: Vec<TestRow>,
}

impl TestRunUi {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub fn push_row(&mut self, qualified_name: impl Into<String>, state: TestRowState, link: Option<FileLineLink>) {
        self.rows.push(TestRow {
            qualified_name: qualified_name.into(),
            link,
            state,
            duration: None,
            failure_detail: None,
        });
    }

    pub fn start_running(&mut self, index: usize) -> io::Result<()> {
        if index >= self.rows.len() {
            return Ok(());
        }
        self.rows[index].state = TestRowState::Running;
        self.rows[index].duration = None;
        beskid_telemetry::clear_stderr_progress()?;
        eprintln!("{}", self.rows[index].qualified_name);
        Ok(())
    }

    pub fn finish_row(
        &mut self,
        index: usize,
        state: TestRowState,
        duration: Duration,
        detail: Option<&str>,
    ) -> io::Result<()> {
        if index >= self.rows.len() {
            return Ok(());
        }
        self.rows[index].state = state;
        self.rows[index].duration = Some(duration);
        if state == TestRowState::Failed {
            self.rows[index].failure_detail = detail.map(str::to_owned);
        }
        beskid_telemetry::clear_stderr_progress()?;
        {
            let row = &self.rows[index];
            let name = maybe_link_label(row.link.as_ref(), &row.qualified_name, true);
            match state {
                TestRowState::Passed => eprintln!("PASS {name}"),
                TestRowState::Failed => eprintln!("FAIL {name}"),
                TestRowState::Skipped => {
                    if let Some(reason) = detail {
                        eprintln!("SKIP {name}: {reason}");
                    } else {
                        eprintln!("SKIP {name}");
                    }
                }
                TestRowState::FilteredOut => eprintln!("FILT {name}"),
                TestRowState::TimedOut => {
                    if let Some(reason) = detail {
                        eprintln!("TIME {name}: {reason}");
                    } else {
                        eprintln!("TIME {name}");
                    }
                }
                TestRowState::Pending | TestRowState::Running => eprintln!("???? {name}"),
            }
            Ok(())
        }
    }

    pub fn print_summary(
        &mut self,
        passed: usize,
        failed: usize,
        skipped: usize,
        filtered_out: usize,
        timed_out: usize,
    ) -> io::Result<()> {
        let summary_line = format!(
            "Result: passed={passed}, failed={failed}, skipped={skipped}, filtered_out={filtered_out}, timed_out={timed_out}"
        );
        beskid_telemetry::clear_stderr_progress()?;
        println!("{summary_line}");
        Ok(())
    }
}

impl Default for TestRunUi {
    fn default() -> Self {
        Self::new()
    }
}
