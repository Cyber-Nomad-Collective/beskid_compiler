//! Live test-run table for `beskid test` (status, duration, name).

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use super::hyperlink::FileLineLink;
use super::terminal::TuiSession;
use super::model::TestReportSummary;

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
}

/// Interactive or plain presenter for a test run.
pub struct TestRunUi {
    plain: bool,
    rows: Vec<TestRow>,
    tui: Option<TuiSession>,
}

impl TestRunUi {
    pub fn new(plain: bool, use_tty: bool) -> Self {
        let interactive = use_tty && !plain && stderr().is_terminal();
        Self {
            plain: !interactive,
            rows: Vec::new(),
            tui: None,
        }
    }

    pub fn push_row(
        &mut self,
        qualified_name: impl Into<String>,
        state: TestRowState,
        link: Option<FileLineLink>,
    ) {
        self.rows.push(TestRow {
            qualified_name: qualified_name.into(),
            link,
            state,
            duration: None,
        });
    }

    pub fn is_plain(&self) -> bool {
        self.plain
    }

    fn ensure_tui(&mut self) -> io::Result<()> {
        if self.plain || self.tui.is_some() {
            return Ok(());
        }
        self.tui = Some(TuiSession::try_open(true)?);
        Ok(())
    }

    pub fn draw_initial(&mut self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        self.ensure_tui()?;
        if let Some(tui) = &mut self.tui {
            let title = format!("Tests ({})", self.rows.len());
            tui.begin_tests(title, self.rows.clone())?;
        }
        Ok(())
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
        if self.plain {
            let name = &self.rows[index].qualified_name;
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
            return Ok(());
        }
        self.redraw()?;
        if state == TestRowState::Failed {
            writeln!(stderr())?;
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
        let summary = TestReportSummary {
            passed,
            failed,
            skipped,
            filtered_out,
        };
        let summary_line = format!(
            "Result: passed={passed}, failed={failed}, skipped={skipped}, filtered_out={filtered_out}"
        );
        if self.plain {
            println!("{summary_line}");
            return Ok(());
        }
        if let Some(tui) = &mut self.tui {
            let title = format!("Tests ({})", self.rows.len());
            let panel = summary.into_command_summary(title);
            tui.show_summary(panel)?;
            tui.suspend()?;
        }
        writeln!(stderr(), "{summary_line}")?;
        Ok(())
    }

    fn redraw(&mut self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        self.ensure_tui()?;
        if let Some(tui) = &mut self.tui {
            tui.update_test_rows(self.rows.clone())?;
        }
        Ok(())
    }
}

fn stderr() -> io::Stderr {
    io::stderr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_mode_skips_tui() {
        let ui = TestRunUi::new(true, false);
        assert!(ui.is_plain());
        assert!(ui.tui.is_none());
    }
}
