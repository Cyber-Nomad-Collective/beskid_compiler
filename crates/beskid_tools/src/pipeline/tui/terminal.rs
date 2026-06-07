//! Ratatui terminal session: alternate screen + TEA dispatch loop hook.

use std::io::{self, Stderr, Write, stderr};

use crossterm::ExecutableCommand;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::app::{Message, Model, update, view};
use super::logger_panel::{init_session_logger, shutdown_session_logger};
use super::test_report::TestReportSummary;
use super::test_table::TestRow;

pub use super::app::PipelineProgress as PipelineViewState;

/// Ratatui session on stderr alternate screen.
pub struct TuiSession {
    terminal: Option<Terminal<CrosstermBackend<Stderr>>>,
    model: Model,
}

impl TuiSession {
    pub fn try_open(interactive: bool) -> io::Result<Self> {
        if !interactive {
            return Ok(Self::try_open_plain());
        }
        enable_raw_mode()?;
        stderr().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stderr());
        let terminal = Terminal::new(backend)?;
        init_session_logger();
        Ok(Self {
            terminal: Some(terminal),
            model: Model::default(),
        })
    }

    pub fn try_open_plain() -> Self {
        Self {
            terminal: None,
            model: Model::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.terminal.is_some()
    }

    pub fn tree_phase_start(&mut self, depth: usize, label: impl Into<String>) -> io::Result<()> {
        self.dispatch(Message::PhaseStart {
            depth,
            label: label.into(),
        })
    }

    pub fn tree_phase_end(
        &mut self,
        depth: usize,
        label: impl Into<String>,
        duration: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(Message::PhaseEnd {
            depth,
            label: label.into(),
            duration: duration.into(),
        })
    }

    pub fn tree_work_unit(
        &mut self,
        depth: usize,
        done: u64,
        total: u64,
        label: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(Message::WorkUnit {
            depth,
            done,
            total,
            label: label.into(),
        })
    }

    pub fn set_pipeline_progress(
        &mut self,
        total_pos: u64,
        total_len: u64,
        total_label: impl Into<String>,
        stage_pos: u64,
        stage_len: u64,
        stage_label: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(Message::SetProgress {
            total_pos,
            total_len,
            total_label: total_label.into(),
            stage_pos,
            stage_len,
            stage_label: stage_label.into(),
        })
    }

    pub fn begin_tests(&mut self, title: impl Into<String>, rows: Vec<TestRow>) -> io::Result<()> {
        self.dispatch(Message::BeginTests {
            title: title.into(),
            rows,
        })
    }

    pub fn update_test_rows(&mut self, rows: Vec<TestRow>) -> io::Result<()> {
        self.dispatch(Message::UpdateTestRows(rows))
    }

    pub fn show_test_report(
        &mut self,
        summary: TestReportSummary,
        title: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(Message::ShowTestReport {
            summary,
            title: title.into(),
        })
    }

    pub fn push_log(&mut self, line: &str) -> io::Result<()> {
        self.dispatch(Message::PushLog(line.to_string()))
    }

    fn dispatch(&mut self, msg: Message) -> io::Result<()> {
        update(&mut self.model, msg);
        self.draw()
    }

    pub fn draw(&mut self) -> io::Result<()> {
        let Some(terminal) = &mut self.terminal else {
            return Ok(());
        };
        terminal.draw(|frame| view(&mut self.model, frame))?;
        Ok(())
    }

    pub fn suspend(&mut self) -> io::Result<()> {
        let Some(terminal) = &mut self.terminal else {
            return Ok(());
        };
        disable_raw_mode()?;
        stderr().execute(LeaveAlternateScreen)?;
        terminal.clear()?;
        shutdown_session_logger();
        self.terminal = None;
        Ok(())
    }

    pub fn finish(mut self, summary: &str) -> io::Result<()> {
        if self.terminal.is_some() {
            self.suspend()?;
        }
        writeln!(stderr(), "{summary}")?;
        stderr().flush()?;
        Ok(())
    }
}
