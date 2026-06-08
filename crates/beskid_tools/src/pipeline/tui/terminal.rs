//! Ratatui terminal session: alternate screen + TEA dispatch loop.

use std::io::{self, Stderr, Write, stderr};
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::ResetColor;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::message::Message;
use super::model::{CommandSummary, Model, TestReportSummary};
use super::nav::NavTarget;
use super::test_table::TestRow;
use super::update::update;
use super::view::view;
use super::widgets::{init_session_logger, shutdown_session_logger};

pub use super::model::PipelineProgress as PipelineViewState;

/// Reset SGR/ANSI attributes on stderr so test output cannot bleed into the next TUI frame.
pub fn reset_stderr_ansi() -> io::Result<()> {
    let mut out = stderr();
    out.execute(ResetColor)?;
    write!(out, "\x1b[0m")?;
    out.flush()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    None,
    Advance,
    Quit,
}

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

    pub fn stage_summary(&mut self, summary: CommandSummary) -> io::Result<()> {
        self.dispatch(Message::StageSummary(summary))
    }

    pub fn mark_compile_complete(&mut self) -> io::Result<()> {
        self.dispatch(Message::CompileComplete)
    }

    pub fn show_tests_screen(&mut self) -> io::Result<()> {
        self.dispatch(Message::ShowTestsScreen)
    }

    pub fn show_summary_screen(&mut self) -> io::Result<()> {
        self.dispatch(Message::ShowSummaryScreen)
    }

    pub fn push_log(&mut self, line: &str) -> io::Result<()> {
        self.dispatch(Message::PushLog(line.to_string()))
    }

    /// Block until the user presses Space to reach `target`, or q/Esc to continue without waiting.
    pub fn wait_for(&mut self, target: NavTarget) -> io::Result<()> {
        if self.terminal.is_none() {
            return Ok(());
        }
        loop {
            self.draw()?;
            if self.reached(target) {
                return Ok(());
            }
            match self.poll_key_action()? {
                KeyAction::Advance if self.reached(target) => return Ok(()),
                KeyAction::Quit => return Ok(()),
                KeyAction::Advance | KeyAction::None => {}
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// After summary is staged: Space/q on summary screen, then suspend.
    pub fn wait_for_dismiss(&mut self) -> io::Result<()> {
        if self.terminal.is_none() {
            return Ok(());
        }
        if !self.model.summary_ready {
            return Ok(());
        }
        if self.model.mode != super::model::Mode::Summary {
            self.show_summary_screen()?;
        }
        loop {
            self.draw()?;
            match self.poll_key_action()? {
                KeyAction::Advance => break,
                KeyAction::Quit => break,
                KeyAction::None => {}
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.suspend()
    }

    fn reached(&self, target: NavTarget) -> bool {
        match target {
            NavTarget::Tests => self.model.mode == super::model::Mode::Tests,
            NavTarget::Summary => self.model.mode == super::model::Mode::Summary,
            NavTarget::Exit => false,
        }
    }

    fn poll_key_action(&mut self) -> io::Result<KeyAction> {
        if !event::poll(Duration::from_millis(0))? {
            return Ok(KeyAction::None);
        }
        let Event::Key(key) = event::read()? else {
            return Ok(KeyAction::None);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(KeyAction::None);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Ok(KeyAction::Quit),
            KeyCode::Char(' ') | KeyCode::Enter => {
                let _ = self.model.advance_once();
                Ok(KeyAction::Advance)
            }
            _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') => {
                Ok(KeyAction::Quit)
            }
            _ => Ok(KeyAction::None),
        }
    }

    /// Poll keyboard input and redraw; Space advances when the next screen is ready.
    pub fn pump_interactive(&mut self) -> io::Result<()> {
        if self.terminal.is_none() {
            return Ok(());
        }
        let _ = self.poll_key_action()?;
        self.draw()
    }

    fn dispatch(&mut self, msg: Message) -> io::Result<()> {
        let mut next = Some(msg);
        while let Some(current) = next.take() {
            next = update(&mut self.model, current);
        }
        self.draw()
    }

    pub fn draw(&mut self) -> io::Result<()> {
        let Some(terminal) = &mut self.terminal else {
            return Ok(());
        };
        reset_stderr_ansi()?;
        terminal.draw(|frame| view(&mut self.model, frame))?;
        Ok(())
    }

    /// Re-enter alternate screen after [`suspend`](Self::suspend), preserving model state.
    pub fn resume(&mut self) -> io::Result<()> {
        if self.terminal.is_some() {
            return Ok(());
        }
        enable_raw_mode()?;
        stderr().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stderr());
        let terminal = Terminal::new(backend)?;
        init_session_logger();
        self.terminal = Some(terminal);
        self.draw()
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
