//! [`CliPipeline`]: the line-oriented output policy of CLI commands (status and detail lines,
//! phase tree, work units and the transient progress line).

use std::borrow::Cow;
use std::env;
use std::io::{IsTerminal, Write, stderr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_pipeline::{
    PipelineEvent, PipelineObserver,
    phases::{FULL_BUILD_PHASE_ORDER, JIT_RUN_PHASE_ORDER, MOD_BUILD_PHASE_ORDER},
};

use super::labels::phase_label;
use super::tui::{
    self, count_severities, format_duration, format_phase_end, format_phase_start, format_severity_summary,
    format_work_unit,
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

/// A step must run this long before the transient progress line appears; faster steps stay silent.
pub const PROGRESS_DELAY: Duration = Duration::from_secs(1);

/// Whether a step that has run for `elapsed` is slow enough for a progress line.
pub fn progress_due(elapsed: Duration) -> bool {
    elapsed >= PROGRESS_DELAY
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
            self.println_detail("No diagnostics.");
            return counts;
        }
        for diagnostic in crate::diagnostics::authored_diagnostics(diagnostics.iter().cloned()) {
            crate::diagnostics::eprint_diagnostic(&diagnostic);
        }
        let _ = stderr().flush();
        self.println_status(format!("Analysis: {}", format_severity_summary(counts)));
        counts
    }

    pub fn finish_prepare_ui(&self, message: impl Into<Cow<'static, str>>) {
        let mut finished = self.prepare_ui_finished.lock().expect("prepare mutex poisoned");
        if *finished {
            return;
        }
        *finished = true;
        self.finish_detail(message);
    }

    pub fn prepare_ui_finished(&self) -> bool {
        *self.prepare_ui_finished.lock().expect("prepare mutex poisoned")
    }

    /// Print the command's status line on stderr (default output and above). `-q` suppresses it,
    /// never diagnostics or command results. A command prints at most one per outcome.
    pub fn println_status(&self, line: impl AsRef<str>) {
        self.halt_progress_bars_for_output();
        if !beskid_telemetry::verbosity().shows_status() {
            return;
        }
        eprintln!("{}", line.as_ref());
    }

    /// Print one pipeline detail line (phase tree, work units, sub-steps) on stderr; only `-v` and
    /// above show these.
    pub fn println_detail(&self, line: impl AsRef<str>) {
        self.halt_progress_bars_for_output();
        if !beskid_telemetry::verbosity().shows_detail() {
            return;
        }
        eprintln!("{}", line.as_ref());
    }

    /// Finish a pipeline stage that is not the command's last: reported with `-v` only.
    pub fn finish_detail(&self, message: impl Into<Cow<'static, str>>) {
        self.flush_pending_work_unit();
        self.println_detail(format!("{} in {}", message.into(), format_duration(self.started_at.elapsed())));
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
        self.println_status(format!("{} in {}", message.into(), format_duration(self.started_at.elapsed())));
        if let Some(summary) = summary {
            for stat in summary.stats {
                self.println_detail(format!("  {}: {}", stat.label, stat.value));
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
            self.println_detail(line);
        }
    }

    /// Time the running top-level step has been going, or the whole command when between steps.
    fn step_elapsed(&self) -> Duration {
        let stack = self.phase_stack.lock().expect("phase mutex poisoned");
        stack.first().map_or_else(|| self.started_at.elapsed(), |entry| entry.started.elapsed())
    }

    fn show_progress(&self, done: u64, total: u64, label: &str) {
        // `plain` is set unless stderr is an interactive terminal; `-q` and `-v` (whose phase lines
        // already narrate the work) never draw the transient line either.
        if self.plain || beskid_telemetry::verbosity() != beskid_telemetry::Verbosity::Normal {
            return;
        }
        if !progress_due(self.step_elapsed()) {
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
        self.println_detail(format_phase_start(depth, self.plain, phase_label(id)));
    }

    fn on_phase_end(&self, id: &'static str) {
        self.flush_pending_work_unit();
        let (depth, duration) = {
            let mut stack = self.phase_stack.lock().expect("phase mutex poisoned");
            let depth = stack.len().saturating_sub(1);
            let duration = stack.pop().map(|entry| entry.started.elapsed()).unwrap_or_default();
            (depth, duration)
        };
        self.println_detail(format_phase_end(depth, self.plain, phase_label(id), &format_duration(duration)));
        if depth == 0 {
            let mut position = self.total_pos.lock().expect("total mutex poisoned");
            *position = position.saturating_add(1);
        }
    }
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
                if !beskid_telemetry::verbosity().shows_detail() {
                    self.show_progress(done, total, &label);
                    return;
                }
                let depth = self.phase_stack.lock().expect("phase mutex poisoned").len().saturating_add(1);
                let line = format_work_unit(depth, self.plain, done, total, &label);
                let emit = {
                    let mut events = self.work_unit_events.lock().expect("work unit mutex poisoned");
                    *events = events.saturating_add(1);
                    // `-vv` reports every work unit; `-v` samples long phases.
                    beskid_telemetry::verbosity() == beskid_telemetry::Verbosity::Debug
                        || *events == 1
                        || events.is_multiple_of(32)
                        || done >= total
                };
                if emit {
                    self.pending_work_unit.lock().expect("work unit mutex poisoned").take();
                    self.println_detail(line);
                } else {
                    *self.pending_work_unit.lock().expect("work unit mutex poisoned") = Some(line);
                }
                self.show_progress(done, total, &label);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_appears_only_after_about_one_second() {
        assert!(!progress_due(Duration::ZERO));
        assert!(!progress_due(Duration::from_millis(999)));
        assert!(progress_due(PROGRESS_DELAY));
        assert!(progress_due(Duration::from_secs(5)));
    }

    #[test]
    fn non_terminal_sessions_never_draw_progress() {
        // `new_with_kind(false, ..)` is the `--plain` / piped-stderr configuration.
        let pipeline = CliPipeline::new_with_kind(false, PipelineProgressKind::FullBuild);
        assert!(pipeline.plain);
        assert!(pipeline.step_elapsed() < PROGRESS_DELAY);
    }
}
