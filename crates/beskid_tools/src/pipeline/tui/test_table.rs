//! Live test-run table for `beskid test` (status, duration, name).

use std::io::{self, Write};
use std::time::Duration;

use super::hyperlink::{FileLineLink, maybe_link_label};
use super::model::TestReportSummary;
use crate::pipeline::CliPipeline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRowState {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
    FilteredOut,
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

/// Interactive or plain presenter for a test run.
pub struct TestRunUi<'a> {
    plain: bool,
    rows: Vec<TestRow>,
    pipeline: Option<&'a CliPipeline>,
}

impl<'a> TestRunUi<'a> {
    /// When `pipeline` is `Some` and not plain, reuses its [`TuiSession`](super::terminal::TuiSession).
    pub fn new(plain: bool, pipeline: Option<&'a CliPipeline>) -> Self {
        let interactive = !plain && pipeline.is_some_and(|pipeline| pipeline.is_spinner_enabled());
        Self { plain: !interactive, rows: Vec::new(), pipeline: if interactive { pipeline } else { None } }
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

    pub fn is_plain(&self) -> bool {
        self.plain
    }

    pub fn draw_initial(&mut self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        let Some(pipeline) = self.pipeline else {
            return Ok(());
        };
        let title = format!("Tests ({})", self.rows.len());
        pipeline.begin_test_run(title, self.rows.clone())
    }

    pub fn start_running(&mut self, index: usize) -> io::Result<()> {
        if index >= self.rows.len() {
            return Ok(());
        }
        if self.plain {
            eprintln!("{}", self.rows[index].qualified_name);
            return Ok(());
        }
        self.rows[index].state = TestRowState::Running;
        self.rows[index].duration = None;
        if !self.plain {
            super::terminal::reset_stderr_ansi()?;
        }
        self.redraw()
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
        if self.plain {
            let row = &self.rows[index];
            let name = maybe_link_label(row.link.as_ref(), &row.qualified_name, false);
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
                TestRowState::Pending | TestRowState::Running => eprintln!("???? {name}"),
            }
            return super::terminal::reset_stderr_ansi();
        }
        super::terminal::reset_stderr_ansi()?;
        self.redraw()?;
        if state == TestRowState::Failed {
            writeln!(io::stderr())?;
        }
        Ok(())
    }

    pub fn print_summary(
        &mut self,
        passed: usize,
        failed: usize,
        skipped: usize,
        filtered_out: usize,
    ) -> io::Result<()> {
        let summary = TestReportSummary { passed, failed, skipped, filtered_out };
        let summary_line =
            format!("Result: passed={passed}, failed={failed}, skipped={skipped}, filtered_out={filtered_out}");
        if self.plain {
            println!("{summary_line}");
            return Ok(());
        }
        if let Some(pipeline) = self.pipeline {
            let title = format!("Tests ({})", self.rows.len());
            pipeline.show_test_summary(summary, title)?;
        }
        writeln!(io::stderr(), "{summary_line}")?;
        Ok(())
    }

    fn redraw(&mut self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        let Some(pipeline) = self.pipeline else {
            return Ok(());
        };
        pipeline.update_test_rows(self.rows.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_mode_skips_tui() {
        let ui = TestRunUi::new(true, None);
        assert!(ui.is_plain());
        assert!(ui.pipeline.is_none());
    }
}
