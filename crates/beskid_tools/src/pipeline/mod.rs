//! Line-oriented CLI [`beskid_pipeline::PipelineObserver`].

pub mod frontend;
mod labels;
pub mod resolve_options;
pub mod tui;

use std::borrow::Cow;
use std::env;
use std::io::{IsTerminal, Write, stderr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{ResolvedInput, ResolvedProject};
use beskid_pipeline::{
    PipelineEvent, PipelineObserver,
    phases::{FULL_BUILD_PHASE_ORDER, JIT_RUN_PHASE_ORDER, MOD_BUILD_PHASE_ORDER},
};
use labels::phase_label;
pub use resolve_options::{
    CliInputPipelineOptions, CliProjectPipelineOptions, CliResolveOptions, FrontendProjectPipelineOptions,
};
use tui::{
    count_severities, format_duration, format_phase_end, format_phase_start, format_severity_summary, format_work_unit,
};

struct PhaseStackEntry {
    started: Instant,
}

/// Which phase budget the progress bar tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineProgressKind {
    FullBuild,
    ModBuild,
    PrepareAndRun,
}

/// Persistent phase lines with a bounded, input-free TTY progress line.
pub struct CliPipeline {
    plain: bool,
    phase_total: u64,
    total_pos: Mutex<u64>,
    prepare_ui_finished: Mutex<bool>,
    started_at: Instant,
    phase_stack: Mutex<Vec<PhaseStackEntry>>,
    work_unit_events: Mutex<u64>,
    pending_work_unit: Mutex<Option<String>>,
}

impl Drop for CliPipeline {
    fn drop(&mut self) {
        self.halt_progress_bars_for_output();
    }
}

pub fn use_cli_spinner(plain: bool) -> bool {
    !plain
        && env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
        && env::var_os("CI").is_none()
        && stderr().is_terminal()
}

impl CliPipeline {
    pub fn new(use_spinner: bool) -> Self {
        Self::new_with_kind(use_spinner, PipelineProgressKind::FullBuild)
    }

    pub fn new_with_kind(use_spinner: bool, kind: PipelineProgressKind) -> Self {
        let phase_total = match kind {
            PipelineProgressKind::FullBuild => FULL_BUILD_PHASE_ORDER.len(),
            PipelineProgressKind::ModBuild => MOD_BUILD_PHASE_ORDER.len(),
            PipelineProgressKind::PrepareAndRun => 4 + JIT_RUN_PHASE_ORDER.len(),
        } as u64;
        Self {
            plain: !use_cli_spinner(!use_spinner),
            phase_total,
            total_pos: Mutex::new(0),
            prepare_ui_finished: Mutex::new(false),
            started_at: Instant::now(),
            phase_stack: Mutex::new(Vec::new()),
            work_unit_events: Mutex::new(0),
            pending_work_unit: Mutex::new(None),
        }
    }

    /// Clear only the current progress line before diagnostics or child output.
    pub fn halt_progress_bars_for_output(&self) {
        let _ = beskid_telemetry::clear_stderr_progress();
    }

    pub fn report_semantic_diagnostics(&self, diagnostics: &[SemanticDiagnostic]) -> tui::SeverityCounts {
        let counts = count_severities(diagnostics);
        self.halt_progress_bars_for_output();
        if diagnostics.is_empty() {
            self.println_session("No diagnostics.");
            return counts;
        }
        for diagnostic in diagnostics {
            eprint!("{}", crate::diagnostics::format_diagnostic(diagnostic));
        }
        let _ = stderr().flush();
        self.println_session(format!("Analysis: {}", format_severity_summary(counts)));
        counts
    }

    pub fn finish_prepare_ui(&self, message: impl Into<Cow<'static, str>>) {
        let mut finished = self.prepare_ui_finished.lock().expect("prepare mutex poisoned");
        if *finished {
            return;
        }
        *finished = true;
        self.finish_session(message);
    }

    pub fn prepare_ui_finished(&self) -> bool {
        *self.prepare_ui_finished.lock().expect("prepare mutex poisoned")
    }

    pub fn println_session(&self, line: impl AsRef<str>) {
        self.halt_progress_bars_for_output();
        eprintln!("{}", line.as_ref());
    }

    pub fn finish_session(&self, message: impl Into<Cow<'static, str>>) {
        self.finish_session_with_summary(message, None);
    }

    pub fn finish_session_with_summary(
        &self,
        message: impl Into<Cow<'static, str>>,
        summary: Option<tui::CommandSummary>,
    ) {
        self.flush_pending_work_unit();
        self.println_session(format!("{} in {}", message.into(), format_duration(self.started_at.elapsed())));
        if let Some(summary) = summary {
            for stat in summary.stats {
                self.println_session(format!("  {}: {}", stat.label, stat.value));
            }
        }
    }

    pub fn finish_build(&self, message: impl Into<Cow<'static, str>>) {
        self.finish_session(message);
    }
    pub fn finish_build_with_summary(&self, message: impl Into<Cow<'static, str>>, summary: tui::CommandSummary) {
        self.finish_session_with_summary(message, Some(summary));
    }
    fn flush_pending_work_unit(&self) {
        let pending = self.pending_work_unit.lock().expect("work unit mutex poisoned").take();
        if let Some(line) = pending {
            self.println_session(line);
        }
    }

    fn show_progress(&self, done: u64, total: u64, label: &str) {
        if self.plain {
            return;
        }
        let width = crossterm::terminal::size().map(|(width, _)| usize::from(width)).unwrap_or(80).saturating_sub(1);
        let bar_width = 16.min(width.saturating_sub(16));
        let filled = ((done.min(total) as u128 * bar_width as u128) / total.max(1) as u128) as usize;
        let phase = (*self.total_pos.lock().expect("total mutex poisoned")).min(self.phase_total);
        let line = format!(
            "[{}{}] {phase}/{} {done}/{total} {label}",
            "=".repeat(filled),
            " ".repeat(bar_width - filled),
            self.phase_total
        );
        // ASCII keeps the bound exact even for wide Unicode work-unit labels.
        let line: String = line
            .chars()
            .map(|character| if character.is_ascii() && !character.is_control() { character } else { '?' })
            .take(width)
            .collect();
        let _ = beskid_telemetry::render_stderr_progress(&line);
    }

    fn on_phase_start(&self, id: &'static str) {
        self.flush_pending_work_unit();
        *self.work_unit_events.lock().expect("work unit mutex poisoned") = 0;
        let depth = {
            let mut stack = self.phase_stack.lock().expect("phase mutex poisoned");
            let depth = stack.len();
            stack.push(PhaseStackEntry { started: Instant::now() });
            depth
        };
        self.println_session(format_phase_start(depth, self.plain, phase_label(id)));
    }

    fn on_phase_end(&self, id: &'static str) {
        self.flush_pending_work_unit();
        let (depth, duration) = {
            let mut stack = self.phase_stack.lock().expect("phase mutex poisoned");
            let depth = stack.len().saturating_sub(1);
            let duration = stack.pop().map(|entry| entry.started.elapsed()).unwrap_or_default();
            (depth, duration)
        };
        self.println_session(format_phase_end(depth, self.plain, phase_label(id), &format_duration(duration)));
        if depth == 0 {
            let mut position = self.total_pos.lock().expect("total mutex poisoned");
            *position = position.saturating_add(1);
        }
    }
}

pub fn resolve_input_with_cli_pipeline(options: CliResolveOptions<'_>) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    resolve_input_with_cli_pipeline_kind(CliInputPipelineOptions {
        resolve: options,
        progress_kind: PipelineProgressKind::FullBuild,
    })
}

pub fn resolve_input_with_cli_pipeline_kind(
    options: CliInputPipelineOptions<'_>,
) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    let CliInputPipelineOptions { resolve, progress_kind } = options;
    let pipeline = Arc::new(CliPipeline::new_with_kind(use_cli_spinner(resolve.plain), progress_kind));
    let resolved = frontend::resolve_input_with_pipeline(resolve, Some(pipeline.as_ref()))?;
    Ok((pipeline, resolved))
}

pub fn resolve_project_with_cli_pipeline(
    options: CliProjectPipelineOptions<'_>,
) -> Result<(Arc<CliPipeline>, ResolvedProject)> {
    let CliProjectPipelineOptions { resolve, unresolved_dependency_policy } = options;
    let pipeline =
        Arc::new(CliPipeline::new_with_kind(use_cli_spinner(resolve.plain), PipelineProgressKind::FullBuild));
    let resolved = frontend::resolve_project_with_pipeline(FrontendProjectPipelineOptions {
        resolve,
        unresolved_dependency_policy,
        pipeline: Some(pipeline.as_ref()),
    })?;
    Ok((pipeline, resolved))
}

impl PipelineObserver for CliPipeline {
    fn on_event(&self, event: PipelineEvent) {
        if self.prepare_ui_finished() {
            return;
        }
        match event {
            PipelineEvent::PhaseStart { id } => self.on_phase_start(id),
            PipelineEvent::PhaseEnd { id } => self.on_phase_end(id),
            PipelineEvent::WorkUnit { id: _, done, total, label } => {
                let depth = self.phase_stack.lock().expect("phase mutex poisoned").len().saturating_add(1);
                let line = format_work_unit(depth, self.plain, done, total, &label);
                let emit = {
                    let mut events = self.work_unit_events.lock().expect("work unit mutex poisoned");
                    *events = events.saturating_add(1);
                    *events == 1 || events.is_multiple_of(32) || done >= total
                };
                if emit {
                    self.pending_work_unit.lock().expect("work unit mutex poisoned").take();
                    self.println_session(line);
                } else {
                    *self.pending_work_unit.lock().expect("work unit mutex poisoned") = Some(line);
                }
                self.show_progress(done, total, &label);
            }
        }
    }
}
