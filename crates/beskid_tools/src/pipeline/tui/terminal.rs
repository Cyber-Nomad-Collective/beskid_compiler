//! Ratatui session handle: forwards messages to the unified shell runtime.

use std::io;
use std::sync::mpsc::Sender;

use crate::tui::session::ShellSession;
use crate::tui::shell::runtime::RuntimeOp;
use crate::tui::shell::state::NavTarget;

use super::model::{CommandSummary, TestReportSummary};
use super::test_table::TestRow;

pub use super::model::PipelineProgress as PipelineViewState;
pub use crate::tui::session::reset_stderr_ansi;

/// Interactive pipeline UI session (unified shell).
pub struct TuiSession {
    inner: ShellSession,
}

impl TuiSession {
    pub fn try_open(interactive: bool) -> io::Result<Self> {
        Ok(Self {
            inner: ShellSession::try_open(interactive)?,
        })
    }

    pub fn try_open_plain() -> Self {
        Self {
            inner: ShellSession::try_open_plain(),
        }
    }

    pub fn try_attach(tx: Sender<RuntimeOp>) -> Self {
        Self {
            inner: ShellSession::try_attach(tx),
        }
    }

    pub fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    pub fn interrupted(&self) -> bool {
        self.inner.interrupted()
    }

    pub fn tree_phase_start(&self, depth: usize, label: impl Into<String>) -> io::Result<()> {
        self.inner.tree_phase_start(depth, label)
    }

    pub fn tree_phase_end(
        &self,
        depth: usize,
        label: impl Into<String>,
        duration: impl Into<String>,
    ) -> io::Result<()> {
        self.inner.tree_phase_end(depth, label, duration)
    }

    pub fn tree_work_unit(
        &self,
        depth: usize,
        done: u64,
        total: u64,
        label: impl Into<String>,
    ) -> io::Result<()> {
        self.inner.tree_work_unit(depth, done, total, label)
    }

    pub fn active_work_unit(
        &self,
        done: u64,
        total: u64,
        label: impl Into<String>,
    ) -> io::Result<()> {
        self.inner.active_work_unit(done, total, label)
    }

    pub fn set_pipeline_progress(
        &self,
        total_pos: u64,
        total_len: u64,
        total_label: impl Into<String>,
        stage_pos: u64,
        stage_len: u64,
        stage_label: impl Into<String>,
    ) -> io::Result<()> {
        self.inner.set_pipeline_progress(
            total_pos,
            total_len,
            total_label,
            stage_pos,
            stage_len,
            stage_label,
        )
    }

    pub fn begin_tests(&self, title: impl Into<String>, rows: Vec<TestRow>) -> io::Result<()> {
        self.inner.begin_tests(title, rows)
    }

    pub fn update_test_rows(&self, rows: Vec<TestRow>) -> io::Result<()> {
        self.inner.update_test_rows(rows)
    }

    pub fn show_test_report(
        &self,
        summary: TestReportSummary,
        title: impl Into<String>,
    ) -> io::Result<()> {
        self.inner.show_test_report(summary, title)
    }

    pub fn stage_summary(&self, summary: CommandSummary) -> io::Result<()> {
        self.inner.stage_summary(summary)
    }

    pub fn mark_compile_complete(&self) -> io::Result<()> {
        self.inner.mark_compile_complete()
    }

    pub fn show_tests_screen(&self) -> io::Result<()> {
        self.inner.show_tests_overlay()
    }

    pub fn show_summary_screen(&self) -> io::Result<()> {
        self.inner.show_summary_overlay()
    }

    pub fn push_log(&self, line: &str) -> io::Result<()> {
        self.inner.push_log(line)
    }

    pub fn wait_for(&self, target: NavTarget) -> io::Result<()> {
        self.inner.wait_for(target)
    }

    pub fn wait_for_dismiss(&self) -> io::Result<()> {
        self.inner.wait_for_dismiss()
    }

    pub fn pump_interactive(&self) -> io::Result<()> {
        self.inner.pump_interactive()
    }

    pub fn draw(&self) -> io::Result<()> {
        self.inner.draw()
    }

    pub fn suspend(&self) -> io::Result<()> {
        self.inner.suspend()
    }

    pub fn resume(&self) -> io::Result<()> {
        self.inner.resume()
    }

    pub fn finish(self, summary: &str) -> io::Result<()> {
        self.inner.finish(summary)
    }

}

