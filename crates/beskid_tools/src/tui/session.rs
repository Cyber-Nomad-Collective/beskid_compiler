//! Public session facade for CLI commands.

use std::io::{self, Write, stderr};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::pipeline::tui::{CommandSummary, TestReportSummary, TestRow};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::interrupt::InterruptFlag;
use crate::tui::shell::runtime::{RuntimeOp, ShellRuntime};
use crate::tui::shell::state::NavTarget;

pub use crate::pipeline::tui::PipelineViewState;

/// Reset SGR/ANSI attributes on stderr so test output cannot bleed into the next TUI frame.
pub fn reset_stderr_ansi() -> io::Result<()> {
    use crossterm::ExecutableCommand;
    use crossterm::style::ResetColor;

    let mut out = stderr();
    out.execute(ResetColor)?;
    write!(out, "\x1b[0m")?;
    out.flush()
}

/// Interactive shell session (unified compile + overlay views).
pub struct ShellSession {
    runtime: Option<ShellRuntime>,
    /// Forward pipeline messages into a running `beskid hi` shell (no nested TUI runtime).
    attached: Option<Sender<RuntimeOp>>,
    interrupt: InterruptFlag,
}

impl ShellSession {
    pub fn try_open(interactive: bool) -> io::Result<Self> {
        if !interactive {
            return Ok(Self {
                runtime: None,
                attached: None,
                interrupt: InterruptFlag::new(),
            });
        }
        let runtime = ShellRuntime::spawn()?;
        let interrupt = runtime.interrupt_flag();
        Ok(Self {
            runtime: Some(runtime),
            attached: None,
            interrupt,
        })
    }

    /// Attach to an existing hi-shell message channel instead of spawning a nested runtime.
    pub fn try_attach(tx: Sender<RuntimeOp>) -> Self {
        Self {
            runtime: None,
            attached: Some(tx),
            interrupt: InterruptFlag::new(),
        }
    }

    pub fn try_open_plain() -> Self {
        Self {
            runtime: None,
            attached: None,
            interrupt: InterruptFlag::new(),
        }
    }

    pub fn interrupted(&self) -> bool {
        self.interrupt.is_set()
    }

    pub fn is_active(&self) -> bool {
        self.runtime.is_some() || self.attached.is_some()
    }

    pub fn is_attached(&self) -> bool {
        self.attached.is_some()
    }

    pub fn tree_phase_start(&self, depth: usize, label: impl Into<String>) -> io::Result<()> {
        self.dispatch(ShellMessage::PhaseStart {
            depth,
            label: label.into(),
        })
    }

    pub fn tree_phase_end(
        &self,
        depth: usize,
        label: impl Into<String>,
        duration: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(ShellMessage::PhaseEnd {
            depth,
            label: label.into(),
            duration: duration.into(),
        })
    }

    pub fn tree_work_unit(
        &self,
        depth: usize,
        done: u64,
        total: u64,
        label: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(ShellMessage::WorkUnit {
            depth,
            done,
            total,
            label: label.into(),
        })
    }

    pub fn active_work_unit(
        &self,
        done: u64,
        total: u64,
        label: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(ShellMessage::ActiveWork {
            done,
            total,
            label: label.into(),
        })
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
        self.dispatch(ShellMessage::SetProgress {
            total_pos,
            total_len,
            total_label: total_label.into(),
            stage_pos,
            stage_len,
            stage_label: stage_label.into(),
        })
    }

    pub fn begin_tests(&self, title: impl Into<String>, rows: Vec<TestRow>) -> io::Result<()> {
        self.dispatch_sync(ShellMessage::BeginTests {
            title: title.into(),
            rows,
        })
    }

    pub fn update_test_rows(&self, rows: Vec<TestRow>) -> io::Result<()> {
        self.dispatch(ShellMessage::UpdateTestRows(rows))
    }

    pub fn show_test_report(
        &self,
        summary: TestReportSummary,
        title: impl Into<String>,
    ) -> io::Result<()> {
        self.dispatch(ShellMessage::ShowTestReport {
            summary,
            title: title.into(),
        })
    }

    pub fn stage_summary(&self, summary: CommandSummary) -> io::Result<()> {
        self.dispatch(ShellMessage::StageSummary(summary))
    }

    pub fn mark_compile_complete(&self) -> io::Result<()> {
        self.dispatch_sync(ShellMessage::CompileComplete)
    }

    pub fn show_tests_overlay(&self) -> io::Result<()> {
        self.set_overlay_visible(OverlayKind::Tests, true)
    }

    pub fn show_summary_overlay(&self) -> io::Result<()> {
        self.set_overlay_visible(OverlayKind::Summary, true)
    }

    pub fn push_log(&self, line: &str) -> io::Result<()> {
        self.dispatch(ShellMessage::PushLog(line.to_string()))
    }

    pub fn wait_for(&self, target: NavTarget) -> io::Result<()> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        runtime.send_wait(|ack| RuntimeOp::WaitFocus { target, ack })
    }

    pub fn wait_for_dismiss(&self) -> io::Result<()> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        runtime.send_wait(RuntimeOp::WaitDismiss)?;
        self.suspend()
    }

    pub fn pump_interactive(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn draw(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn suspend(&self) -> io::Result<()> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        runtime.send_wait(RuntimeOp::Suspend)
    }

    pub fn resume(&self) -> io::Result<()> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        runtime.send_wait(RuntimeOp::Resume)
    }

    pub fn finish(self, summary: &str) -> io::Result<()> {
        writeln!(stderr(), "{summary}")?;
        stderr().flush()?;
        Ok(())
    }

    pub fn show_pckg_overlay(&self) -> io::Result<()> {
        self.set_overlay_visible(OverlayKind::Pckg, true)
    }

    pub fn show_templates_overlay(&self) -> io::Result<()> {
        self.set_overlay_visible(OverlayKind::Templates, true)
    }

    pub fn enter_project_wizard(&self) -> io::Result<()> {
        self.dispatch_sync(ShellMessage::EnterProjectWizard)
    }

    /// Standalone new-project experience: templates overlay with registry download.
    pub fn run_project_wizard() -> io::Result<()> {
        let session = Self::try_open(true)?;
        session.enter_project_wizard()?;
        while session.is_active() && !session.interrupted() {
            std::thread::sleep(Duration::from_millis(120));
        }
        Ok(())
    }

    fn set_overlay_visible(&self, kind: OverlayKind, visible: bool) -> io::Result<()> {
        if let Some(runtime) = &self.runtime {
            let (ack_tx, ack_rx) = std::sync::mpsc::channel();
            runtime.send(RuntimeOp::SetOverlayVisible {
                kind,
                visible,
                ack: Some(ack_tx),
            })?;
            ack_rx
                .recv()
                .map_err(|_| io::Error::other("tui overlay update interrupted"))?;
            return Ok(());
        }
        if let Some(tx) = &self.attached {
            tx.send(RuntimeOp::SetOverlayVisible {
                kind,
                visible,
                ack: None,
            })
            .map_err(|_| io::Error::other("hi shell message channel closed"))?;
        }
        Ok(())
    }

    fn dispatch(&self, msg: ShellMessage) -> io::Result<()> {
        if let Some(runtime) = &self.runtime {
            runtime.send_update(msg)?;
        } else if let Some(tx) = &self.attached {
            tx.send(RuntimeOp::Update(msg))
                .map_err(|_| io::Error::other("hi shell message channel closed"))?;
        }
        Ok(())
    }

    fn dispatch_sync(&self, msg: ShellMessage) -> io::Result<()> {
        if let Some(runtime) = &self.runtime {
            let (ack_tx, ack_rx) = std::sync::mpsc::channel();
            runtime.send(RuntimeOp::UpdateAndAck(msg, ack_tx))?;
            ack_rx
                .recv()
                .map_err(|_| io::Error::other("tui runtime update interrupted"))?;
            return Ok(());
        }
        self.dispatch(msg)
    }
}

impl Drop for ShellSession {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            let _ = runtime.shutdown();
        }
    }
}
