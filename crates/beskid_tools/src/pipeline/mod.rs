//! CLI [`beskid_pipeline::PipelineObserver`] with plain lines or a Ratatui build UI.

pub mod frontend;

mod labels;
pub mod tui;

use std::borrow::Cow;
use std::env;
use std::io::{self, IsTerminal, Write, stderr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_analysis::services::{ResolvedInput, ResolvedProject};
use beskid_pipeline::{
    PipelineEvent, PipelineObserver,
    phases::{FULL_BUILD_PHASE_ORDER, JIT_RUN_PHASE_ORDER, MOD_BUILD_PHASE_ORDER},
};

use labels::{phase_label, sub_phase_index};
use tui::{
    count_severities, format_duration, format_phase_end, format_phase_start, format_severity_summary,
    format_work_unit, TestReportSummary, TestRow, TuiSession,
};

const WORK_UNIT_UI_MIN_INTERVAL: Duration = Duration::from_millis(120);
const WORK_UNIT_UI_BURST_INTERVAL: u64 = 32;

struct WorkUnitThrottleState {
    last_emit: Option<Instant>,
    work_unit_events: u64,
    pending_msg: Option<String>,
}

struct PhaseStackEntry {
    id: &'static str,
    started: Instant,
}

impl WorkUnitThrottleState {
    fn reset_for_phase_boundary(&mut self) {
        self.last_emit = None;
        self.work_unit_events = 0;
        self.pending_msg = None;
    }

    fn should_emit_work_unit(&mut self, msg: String, now: Instant) -> bool {
        self.work_unit_events = self.work_unit_events.wrapping_add(1);
        self.pending_msg = Some(msg);
        let due_time = self
            .last_emit
            .map(|t| now.duration_since(t) >= WORK_UNIT_UI_MIN_INTERVAL)
            .unwrap_or(true);
        let due_burst = self
            .work_unit_events
            .is_multiple_of(WORK_UNIT_UI_BURST_INTERVAL);
        if due_time || due_burst {
            self.last_emit = Some(now);
            true
        } else {
            false
        }
    }

    fn take_pending_message(&mut self) -> Option<String> {
        self.pending_msg.take()
    }
}

/// Which phase budget the top progress bar tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineProgressKind {
    /// Full `beskid build` pipeline (resolve through link).
    FullBuild,
    /// Resolve and rebuild a compiler-mod AOT artifact.
    ModBuild,
    /// Resolve/materialize plus a single JIT lower/run (test, run, clif).
    PrepareAndRun,
}

/// CLI adapter: maps [`PipelineEvent`] to Ratatui or plain `eprintln`.
pub struct CliPipeline {
    plain: bool,
    phase_total: u64,
    total_pos: Mutex<u64>,
    prepare_ui_finished: Mutex<bool>,
    tui_suspended: Mutex<bool>,
    tui: Mutex<TuiSession>,
    started_at: Instant,
    phase_stack: Mutex<Vec<PhaseStackEntry>>,
    work_unit_throttle: Mutex<WorkUnitThrottleState>,
}

impl Drop for CliPipeline {
    fn drop(&mut self) {
        if let Ok(mut tui) = self.tui.lock() {
            let _ = tui.suspend();
        }
    }
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

pub fn use_cli_spinner(plain: bool) -> bool {
    !plain && !no_color_requested() && stderr().is_terminal()
}

impl CliPipeline {
    pub fn new(use_spinner: bool) -> Self {
        Self::new_with_kind(use_spinner, PipelineProgressKind::FullBuild)
    }

    pub fn new_with_kind(use_spinner: bool, kind: PipelineProgressKind) -> Self {
        let tty = stderr().is_terminal();
        let plain = !use_spinner || !tty;
        let phase_total = match kind {
            PipelineProgressKind::FullBuild => FULL_BUILD_PHASE_ORDER.len(),
            PipelineProgressKind::ModBuild => MOD_BUILD_PHASE_ORDER.len(),
            PipelineProgressKind::PrepareAndRun => 4 + JIT_RUN_PHASE_ORDER.len(),
        } as u64;
        let tui = TuiSession::try_open(!plain).unwrap_or_else(|err| {
            eprintln!("warning: terminal UI unavailable ({err}); falling back to plain output");
            TuiSession::try_open_plain()
        });
        Self {
            plain,
            phase_total,
            total_pos: Mutex::new(0),
            prepare_ui_finished: Mutex::new(false),
            tui_suspended: Mutex::new(plain),
            tui: Mutex::new(tui),
            started_at: Instant::now(),
            phase_stack: Mutex::new(Vec::new()),
            work_unit_throttle: Mutex::new(WorkUnitThrottleState {
                last_emit: None,
                work_unit_events: 0,
                pending_msg: None,
            }),
        }
    }

    /// Re-enter alternate screen after [`halt_progress_bars_for_output`](Self::halt_progress_bars_for_output).
    pub fn resume_after_output(&self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        let mut suspended = self
            .tui_suspended
            .lock()
            .expect("tui_suspended mutex poisoned");
        if !*suspended {
            return Ok(());
        }
        if let Ok(mut tui) = self.tui.lock() {
            tui.resume()?;
        }
        *suspended = false;
        Ok(())
    }

    /// Transition the shared shell from pipeline prepare to live test execution.
    pub fn begin_test_run(
        &self,
        title: impl Into<String>,
        rows: Vec<TestRow>,
    ) -> io::Result<()> {
        self.resume_after_output()?;
        self.with_tui_result(|tui| tui.begin_tests(title, rows))
    }

    /// Refresh test rows in the shared shell.
    pub fn update_test_rows(&self, rows: Vec<TestRow>) -> io::Result<()> {
        self.with_tui_result(|tui| tui.update_test_rows(rows))
    }

    /// Show the test-outcome summary chart in the shared shell (staged until Space).
    pub fn show_test_summary(
        &self,
        summary: TestReportSummary,
        title: impl Into<String>,
    ) -> io::Result<()> {
        self.with_tui_result(|tui| {
            tui.show_test_report(summary, title)
        })
    }

    /// Mark compile/prepare complete; pipeline tree remains until Space.
    pub fn mark_compile_complete(&self) -> io::Result<()> {
        self.with_tui_result(|tui| tui.mark_compile_complete())
    }

    /// Block until Space opens the test screen (q/Esc skips).
    pub fn wait_for_tests_screen(&self) -> io::Result<()> {
        self.with_tui_result(|tui| tui.wait_for(tui::NavTarget::Tests))
    }

    /// Block until Space opens the summary screen (q/Esc skips).
    pub fn wait_for_summary_screen(&self) -> io::Result<()> {
        self.with_tui_result(|tui| tui.wait_for(tui::NavTarget::Summary))
    }

    /// Pump keyboard events between long-running steps (Space still advances when ready).
    pub fn pump_interactive(&self) -> io::Result<()> {
        self.reset_after_test()
    }

    /// Clear stray ANSI styling from test output, then refresh the TUI shell.
    pub fn reset_after_test(&self) -> io::Result<()> {
        tui::reset_stderr_ansi()?;
        if self.plain {
            return Ok(());
        }
        self.with_tui_result(|tui| tui.pump_interactive())
    }

    /// Block on the summary screen until Space/q, then leave alternate screen.
    pub fn wait_for_dismiss(&self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        if let Ok(mut tui) = self.tui.lock() {
            tui.wait_for_dismiss()?;
        }
        let mut suspended = self
            .tui_suspended
            .lock()
            .expect("tui_suspended mutex poisoned");
        *suspended = true;
        Ok(())
    }

    /// Leave alternate screen before writing to stderr (avoids TTY deadlocks with miette).
    pub fn halt_progress_bars_for_output(&self) {
        if self.plain {
            return;
        }
        let mut suspended = self
            .tui_suspended
            .lock()
            .expect("tui_suspended mutex poisoned");
        if *suspended {
            return;
        }
        *suspended = true;
        self.flush_pending_work_unit_ui();
        if let Ok(mut tui) = self.tui.lock() {
            let _ = tui.suspend();
        }
    }

    fn tui_active(&self) -> bool {
        !self.tui_suspended() && !self.plain && self.tui.lock().is_ok_and(|t| t.is_active())
    }

    fn should_use_tui(&self) -> bool {
        self.tui_active()
    }

    fn tui_suspended(&self) -> bool {
        *self
            .tui_suspended
            .lock()
            .expect("tui_suspended mutex poisoned")
    }

    /// Print semantic diagnostics (suspending the TUI when needed) and return severity counts.
    pub fn report_semantic_diagnostics(
        &self,
        diagnostics: &[SemanticDiagnostic],
    ) -> tui::SeverityCounts {
        let counts = count_severities(diagnostics);
        self.halt_progress_bars_for_output();
        if diagnostics.is_empty() {
            self.println_session("No diagnostics.");
            return counts;
        }
        for diagnostic in diagnostics {
            let rendered = crate::diagnostics::format_diagnostic(diagnostic);
            eprint!("{rendered}");
        }
        let _ = stderr().flush();
        self.println_session(format!("Analysis: {}", format_severity_summary(counts)));
        counts
    }

    /// Stop animated progress after resolve/analysis so JIT/test work does not fight the TUI.
    pub fn finish_prepare_ui(&self, message: impl Into<Cow<'static, str>>) {
        let mut finished = self
            .prepare_ui_finished
            .lock()
            .expect("prepare_ui_finished mutex poisoned");
        if *finished {
            return;
        }
        *finished = true;
        self.finish_session(message);
    }

    pub fn prepare_ui_finished(&self) -> bool {
        *self
            .prepare_ui_finished
            .lock()
            .expect("prepare_ui_finished mutex poisoned")
    }

    pub fn println_session(&self, line: impl AsRef<str>) {
        let line = line.as_ref();
        if self.plain || self.tui_suspended() || self.prepare_ui_finished() {
            tracing::info!(target: "beskid.tools.pipeline", "{line}");
            eprintln!("{line}");
        } else if let Ok(mut tui) = self.tui.lock() {
            let _ = tui.push_log(line);
        }
    }

    pub fn finish_session(&self, message: impl Into<Cow<'static, str>>) {
        self.finish_session_with_summary(message, None);
    }

    pub fn finish_session_with_summary(
        &self,
        message: impl Into<Cow<'static, str>>,
        summary: Option<tui::CommandSummary>,
    ) {
        let msg = message.into().into_owned();
        self.flush_pending_work_unit_ui();
        let elapsed = self.started_at.elapsed();
        let headline = format!("{msg} in {}", format_duration(elapsed));
        if !self.plain {
            if let Ok(mut tui) = self.tui.lock() {
                if tui.is_active() {
                    let panel = summary
                        .unwrap_or_else(|| tui::CommandSummary::plain("Result", headline.clone()));
                    let _ = tui.stage_summary(panel);
                    let _ = tui.wait_for(tui::NavTarget::Summary);
                    let _ = tui.wait_for_dismiss();
                }
            }
            let mut suspended = self
                .tui_suspended
                .lock()
                .expect("tui_suspended mutex poisoned");
            *suspended = true;
        }
        eprintln!("{headline}");
    }

    pub fn finish_build(&self, message: impl Into<Cow<'static, str>>) {
        self.finish_session(message);
    }

    pub fn finish_build_with_summary(
        &self,
        message: impl Into<Cow<'static, str>>,
        summary: tui::CommandSummary,
    ) {
        self.finish_session_with_summary(message, Some(summary));
    }

    pub fn is_spinner_enabled(&self) -> bool {
        !self.plain
    }

    fn flush_pending_work_unit_ui(&self) {
        let pending = {
            let mut t = self
                .work_unit_throttle
                .lock()
                .expect("cli pipeline throttle mutex poisoned");
            t.take_pending_message()
        };
        let Some(msg) = pending else {
            return;
        };
        if !self.should_use_tui() {
            eprintln!("{msg}");
        }
    }

    fn parent_phase_id(&self, depth: usize) -> Option<&'static str> {
        let stack = self.phase_stack.lock().ok()?;
        if depth == 0 {
            return None;
        }
        stack.get(depth.saturating_sub(1)).map(|entry| entry.id)
    }

    fn stage_progress(&self, depth: usize, id: &'static str) -> (u64, u64, String) {
        let label = phase_label(id).to_owned();
        if depth == 0 {
            return (0, 1, label);
        }
        if let Some(parent_id) = self.parent_phase_id(depth)
            && let Some((index, total)) = sub_phase_index(parent_id, id)
        {
            return (index as u64, total as u64, label);
        }
        (0, 1, label)
    }

    fn refresh_progress_bars(
        &self,
        stage_pos: u64,
        stage_len: u64,
        stage_label: &str,
    ) {
        if !self.tui_active() {
            return;
        }
        let total_pos = *self.total_pos.lock().expect("total_pos mutex poisoned");
        if let Ok(mut tui) = self.tui.lock() {
            let _ = tui.set_pipeline_progress(
                total_pos,
                self.phase_total,
                "Pipeline",
                stage_pos,
                stage_len,
                stage_label,
            );
        }
    }

    fn current_phase_depth(&self) -> usize {
        self.phase_stack
            .lock()
            .map(|stack| stack.len())
            .unwrap_or(0)
    }

    fn emit_tree_line(&self, line: impl AsRef<str>) {
        let line = line.as_ref();
        if !self.should_use_tui() {
            eprintln!("{line}");
        }
    }

    fn with_tui<F>(&self, f: F)
    where
        F: FnOnce(&mut TuiSession) -> io::Result<()>,
    {
        let _ = self.with_tui_result(f);
    }

    fn with_tui_result<F>(&self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut TuiSession) -> io::Result<()>,
    {
        if !self.should_use_tui() {
            return Ok(());
        }
        if let Ok(mut tui) = self.tui.lock() {
            f(&mut tui)?;
        }
        Ok(())
    }

    fn emit_work_unit_if_due(
        &self,
        msg: String,
        depth: usize,
        done: u64,
        total: u64,
        label: &str,
    ) {
        let now = Instant::now();
        let emit = {
            let mut t = self
                .work_unit_throttle
                .lock()
                .expect("cli pipeline throttle mutex poisoned");
            t.should_emit_work_unit(msg.clone(), now)
        };
        if emit {
            self.refresh_progress_bars(done, total.max(1), label);
            if !self.should_use_tui() {
                eprintln!("{msg}");
            } else {
                self.with_tui(|tui| tui.tree_work_unit(depth, done, total, label));
            }
        }
    }

    fn on_phase_start(&self, id: &'static str) {
        self.flush_pending_work_unit_ui();
        if let Ok(mut t) = self.work_unit_throttle.lock() {
            t.reset_for_phase_boundary();
        }
        let depth = self.current_phase_depth();
        if let Ok(mut stack) = self.phase_stack.lock() {
            stack.push(PhaseStackEntry {
                id,
                started: Instant::now(),
            });
        }
        let label = phase_label(id);
        let line = format_phase_start(depth, self.plain, label);
        let (stage_pos, stage_len, _) = self.stage_progress(depth, id);
        self.refresh_progress_bars(stage_pos, stage_len, label);
        if !self.should_use_tui() {
            if !line.is_empty() {
                eprintln!("{line}");
            }
        } else {
            self.with_tui(|tui| tui.tree_phase_start(depth, label));
        }
    }

    fn on_phase_end(&self, id: &'static str) {
        self.flush_pending_work_unit_ui();
        if let Ok(mut t) = self.work_unit_throttle.lock() {
            t.reset_for_phase_boundary();
        }
        let (depth, duration) = {
            let mut stack = self
                .phase_stack
                .lock()
                .expect("cli pipeline phase stack mutex poisoned");
            let depth = stack.len().saturating_sub(1);
            let duration = stack
                .pop()
                .map(|entry| entry.started.elapsed())
                .unwrap_or_default();
            (depth, duration)
        };
        let label = phase_label(id);
        let duration_text = format_duration(duration);
        let line = format_phase_end(depth, self.plain, label, &duration_text);
        if !self.should_use_tui() {
            eprintln!("{line}");
        } else {
            self.with_tui(|tui| tui.tree_phase_end(depth, label, duration_text));
        }
        if depth == 0 {
            if let Ok(mut total_pos) = self.total_pos.lock() {
                *total_pos = total_pos.saturating_add(1);
            }
            let (stage_pos, stage_len, _) = self.stage_progress(depth, id);
            self.refresh_progress_bars(stage_pos.saturating_add(1), stage_len, label);
        } else if let Some(parent_id) = self.parent_phase_id(depth)
            && let Some((index, total)) = sub_phase_index(parent_id, id)
        {
            self.refresh_progress_bars((index as u64).saturating_add(1), total as u64, label);
        }
    }
}

pub fn resolve_input_with_cli_pipeline(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    plain: bool,
) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    resolve_input_with_cli_pipeline_kind(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        plain,
        PipelineProgressKind::FullBuild,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_input_with_cli_pipeline_kind(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    plain: bool,
    progress_kind: PipelineProgressKind,
) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    let pipeline_ui = Arc::new(CliPipeline::new_with_kind(
        use_cli_spinner(plain),
        progress_kind,
    ));
    let resolved = frontend::resolve_input_with_pipeline(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        Some(pipeline_ui.as_ref()),
    )?;
    Ok((pipeline_ui, resolved))
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_project_with_cli_pipeline(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    plain: bool,
    unresolved_dependency_policy: UnresolvedDependencyPolicy,
) -> Result<(Arc<CliPipeline>, ResolvedProject)> {
    let pipeline_ui = Arc::new(CliPipeline::new_with_kind(
        use_cli_spinner(plain),
        PipelineProgressKind::FullBuild,
    ));
    let resolved = frontend::resolve_project_with_pipeline(
        input,
        project,
        target,
        workspace_member,
        frozen,
        locked,
        unresolved_dependency_policy,
        Some(pipeline_ui.as_ref()),
    )?;
    Ok((pipeline_ui, resolved))
}

impl PipelineObserver for CliPipeline {
    fn on_event(&self, event: PipelineEvent) {
        if self.prepare_ui_finished() {
            return;
        }
        match event {
            PipelineEvent::PhaseStart { id } => self.on_phase_start(id),
            PipelineEvent::PhaseEnd { id } => self.on_phase_end(id),
            PipelineEvent::WorkUnit {
                id: _,
                done,
                total,
                label,
            } => {
                let depth = self.current_phase_depth().saturating_add(1);
                let msg = format_work_unit(depth, self.plain, done, total, &label);
                self.emit_work_unit_if_due(msg, depth, done, total, &label);
            }
        }
    }
}
