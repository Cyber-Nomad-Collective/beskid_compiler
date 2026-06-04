//! CLI [`beskid_pipeline::PipelineObserver`] with plain lines or an interactive build TUI.

mod labels;
pub mod tui;

use std::borrow::Cow;
use std::env;
use std::io::{IsTerminal, Write, stderr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_analysis::services::{ResolvedInput, ResolvedProject};
use beskid_pipeline::{
    PipelineEvent, PipelineObserver,
    phases::{FULL_BUILD_PHASE_ORDER, JIT_RUN_PHASE_ORDER, MOD_BUILD_PHASE_ORDER},
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use labels::phase_label;
use tui::{count_severities, format_duration, format_severity_summary};

const WORK_UNIT_UI_MIN_INTERVAL: Duration = Duration::from_millis(120);
const WORK_UNIT_UI_BURST_INTERVAL: u64 = 32;

struct WorkUnitThrottleState {
    last_emit: Option<Instant>,
    work_unit_events: u64,
    pending_msg: Option<String>,
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

/// Which phase budget the step progress bar tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineProgressKind {
    /// Full `beskid build` pipeline (resolve through link).
    FullBuild,
    /// Resolve and rebuild a compiler-mod AOT artifact.
    ModBuild,
    /// Resolve/materialize plus a single JIT lower/run (test, run, clif).
    PrepareAndRun,
}

/// CLI adapter: maps [`PipelineEvent`] to indicatif or plain `eprintln`.
pub struct CliPipeline {
    plain: bool,
    prepare_ui_finished: Mutex<bool>,
    progress_bars_halted: Mutex<bool>,
    multi: MultiProgress,
    step_bar: Option<ProgressBar>,
    work_bar: Option<ProgressBar>,
    started_at: Instant,
    phase_started_at: Mutex<Option<Instant>>,
    work_unit_throttle: Mutex<WorkUnitThrottleState>,
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

pub fn use_cli_spinner(plain: bool) -> bool {
    !plain && !no_color_requested() && stderr().is_terminal()
}

fn step_bar_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.green} [{bar:36.cyan/blue}] {pos:>2}/{len:2} {wide_msg}",
    )
    .expect("step progress template")
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn work_bar_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.blue.dim} {msg:.dim}")
        .expect("work unit template")
        .tick_strings(&["·", "∙", "•"])
}

impl CliPipeline {
    pub fn new(use_spinner: bool) -> Self {
        Self::new_with_kind(use_spinner, PipelineProgressKind::FullBuild)
    }

    pub fn new_with_kind(use_spinner: bool, kind: PipelineProgressKind) -> Self {
        let tty = stderr().is_terminal();
        let plain = !use_spinner || !tty;
        let multi = MultiProgress::new();
        let phase_count = match kind {
            PipelineProgressKind::FullBuild => FULL_BUILD_PHASE_ORDER.len(),
            PipelineProgressKind::ModBuild => MOD_BUILD_PHASE_ORDER.len(),
            PipelineProgressKind::PrepareAndRun => {
                // Resolve/materialize (4) + semantic gate + one JIT path.
                4 + JIT_RUN_PHASE_ORDER.len()
            }
        };
        let (step_bar, work_bar) = if plain {
            (None, None)
        } else {
            let steps = multi.add(ProgressBar::new(phase_count as u64));
            steps.set_style(step_bar_style());
            steps.enable_steady_tick(Duration::from_millis(100));
            steps.set_message("Starting…");
            let work = multi.add(ProgressBar::new_spinner());
            work.set_style(work_bar_style());
            work.enable_steady_tick(Duration::from_millis(120));
            work.set_message("");
            (Some(steps), Some(work))
        };
        Self {
            plain,
            prepare_ui_finished: Mutex::new(false),
            progress_bars_halted: Mutex::new(false),
            multi,
            step_bar,
            work_bar,
            started_at: Instant::now(),
            phase_started_at: Mutex::new(None),
            work_unit_throttle: Mutex::new(WorkUnitThrottleState {
                last_emit: None,
                work_unit_events: 0,
                pending_msg: None,
            }),
        }
    }

    /// Stop indicatif before writing to stderr (avoids TTY deadlocks with miette).
    pub fn halt_progress_bars_for_output(&self) {
        if self.plain {
            return;
        }
        let mut halted = self
            .progress_bars_halted
            .lock()
            .expect("progress_bars_halted mutex poisoned");
        if *halted {
            return;
        }
        *halted = true;
        self.flush_pending_work_unit_ui();
        if let Some(work) = &self.work_bar {
            work.finish_and_clear();
        }
        if let Some(steps) = &self.step_bar {
            steps.finish_and_clear();
        }
        let _ = self.multi.clear();
    }

    fn progress_bars_active(&self) -> bool {
        !self.progress_bars_halted() && self.step_bar.as_ref().is_some_and(|bar| !bar.is_finished())
    }

    fn progress_bars_halted(&self) -> bool {
        *self
            .progress_bars_halted
            .lock()
            .expect("progress_bars_halted mutex poisoned")
    }

    /// Print semantic diagnostics (suspending progress bars when needed) and return severity counts.
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
            let rendered = crate::errors::format_diagnostic(diagnostic);
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
        if self.plain || self.progress_bars_halted() || self.prepare_ui_finished() {
            eprintln!("{line}");
        } else {
            let _ = self.multi.println(line);
        }
    }

    pub fn finish_session(&self, message: impl Into<Cow<'static, str>>) {
        let msg = message.into().into_owned();
        self.flush_pending_work_unit_ui();
        let elapsed = self.started_at.elapsed();
        let summary = format!("{msg} in {}", format_duration(elapsed));

        let bars_active = self.step_bar.as_ref().is_some_and(|bar| !bar.is_finished());
        if let Some(work) = &self.work_bar
            && !work.is_finished()
        {
            work.finish_and_clear();
        }
        if let Some(bar) = &self.step_bar
            && bars_active
        {
            bar.finish_with_message(summary);
            return;
        }
        eprintln!("{summary}");
    }

    pub fn finish_build(&self, message: impl Into<Cow<'static, str>>) {
        self.finish_session(message);
    }

    pub fn is_spinner_enabled(&self) -> bool {
        self.step_bar.is_some()
    }

    fn flush_pending_work_unit_ui(&self) {
        let msg = {
            let mut t = self
                .work_unit_throttle
                .lock()
                .expect("cli pipeline throttle mutex poisoned");
            t.take_pending_message()
        };
        let Some(msg) = msg else {
            return;
        };
        if self.plain {
            eprintln!("    {msg}");
        } else if let Some(work) = &self.work_bar {
            work.set_message(msg);
        }
    }

    fn emit_work_unit_if_due(&self, msg: String) {
        let now = Instant::now();
        let emit = {
            let mut t = self
                .work_unit_throttle
                .lock()
                .expect("cli pipeline throttle mutex poisoned");
            t.should_emit_work_unit(msg, now)
        };
        if emit {
            self.flush_pending_work_unit_ui();
        }
    }

    fn on_phase_start(&self, id: &'static str) {
        self.flush_pending_work_unit_ui();
        if let Ok(mut t) = self.work_unit_throttle.lock() {
            t.reset_for_phase_boundary();
        }
        if let Ok(mut started) = self.phase_started_at.lock() {
            *started = Some(Instant::now());
        }
        let label = phase_label(id);
        if self.plain || !self.progress_bars_active() {
            eprintln!("→ {label}");
        } else if let Some(steps) = &self.step_bar {
            steps.set_message(label.to_owned());
        }
    }

    fn on_phase_end(&self, id: &'static str) {
        self.flush_pending_work_unit_ui();
        if let Ok(mut t) = self.work_unit_throttle.lock() {
            t.reset_for_phase_boundary();
        }
        let label = phase_label(id).to_owned();
        let duration = self
            .phase_started_at
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
            .map(|start| start.elapsed())
            .unwrap_or_default();
        if self.plain || !self.progress_bars_active() {
            eprintln!("✓ {label} ({})", format_duration(duration));
        } else {
            self.println_session(format!("  ✓ {} ({})", label, format_duration(duration)));
            if let Some(steps) = &self.step_bar {
                steps.inc(1);
                steps.set_message(label);
            }
        }
        if self.progress_bars_active()
            && let Some(work) = &self.work_bar
        {
            work.set_message("");
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
    let resolved = crate::frontend::resolve_input_with_pipeline(
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
    let resolved = crate::frontend::resolve_project_with_pipeline(
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
                id,
                done,
                total,
                label,
            } => {
                let msg = format!("{id} [{done}/{total}] {label}");
                self.emit_work_unit_if_due(msg);
            }
        }
    }
}
