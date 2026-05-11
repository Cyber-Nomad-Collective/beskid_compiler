//! CLI [`beskid_pipeline::PipelineObserver`] that maps [`beskid_pipeline::PipelineEvent`] values to stderr (spinner or plain lines).

use std::borrow::Cow;
use std::env;
use std::io::{IsTerminal, stderr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use beskid_analysis::projects::UnresolvedDependencyPolicy;
use beskid_analysis::services::{ResolvedInput, ResolvedProject};
use beskid_pipeline::{PipelineEvent, PipelineObserver};
use indicatif::{ProgressBar, ProgressStyle};

use crate::frontend;

/// Minimum time between [`PipelineEvent::WorkUnit`] spinner / plain line updates.
const WORK_UNIT_UI_MIN_INTERVAL: Duration = Duration::from_millis(120);
/// Also emit a work-unit line at least every N events when the pipeline fires faster than the interval.
const WORK_UNIT_UI_BURST_INTERVAL: u64 = 32;

/// CLI adapter: maps [`PipelineEvent`] to indicatif or plain `eprintln`.
pub struct CliPipeline {
    /// When true, use line-only messages (no spinner).
    plain: bool,
    spinner: Option<ProgressBar>,
    work_unit_throttle: Mutex<WorkUnitThrottleState>,
}

#[derive(Default)]
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

    /// Returns whether to emit `msg` now; may store `msg` as pending for a later forced flush.
    fn should_emit_work_unit(&mut self, msg: String, now: Instant) -> bool {
        self.work_unit_events = self.work_unit_events.wrapping_add(1);
        self.pending_msg = Some(msg);
        let due_time = self
            .last_emit
            .map(|t| now.duration_since(t) >= WORK_UNIT_UI_MIN_INTERVAL)
            .unwrap_or(true);
        let due_burst = self.work_unit_events % WORK_UNIT_UI_BURST_INTERVAL == 0;
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

/// `NO_COLOR` set to any non-empty value disables spinner styling (same effective path as `--plain` for progress).
fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

/// Whether stderr should show the animated spinner for pipeline progress.
pub(crate) fn use_cli_spinner(plain: bool) -> bool {
    !plain && !no_color_requested() && stderr().is_terminal()
}

impl CliPipeline {
    /// `use_spinner`: animated progress when stderr is a TTY, `NO_COLOR` is unset/empty, and not plain mode.
    pub fn new(use_spinner: bool) -> Self {
        let tty = stderr().is_terminal();
        let plain = !use_spinner || !tty;
        let spinner = if !plain {
            let bar = ProgressBar::new_spinner();
            let style = ProgressStyle::with_template("{spinner:.blue} {msg}")
                .expect("cli pipeline spinner template")
                .tick_strings(&["|", "/", "-", "\\", "=", "-", "\\", "/"]);
            bar.set_style(style);
            bar.enable_steady_tick(Duration::from_millis(90));
            Some(bar)
        } else {
            None
        };
        Self {
            plain,
            spinner,
            work_unit_throttle: Mutex::new(WorkUnitThrottleState::default()),
        }
    }

    /// Print workspace summary and compile-plan edges for a resolved input (debug-style).
    pub fn print_project_graph(resolved: &ResolvedInput) {
        if let Some(ws) = &resolved.workspace_summary {
            println!("Workspace: {}", ws.workspace_manifest_path.display());
            println!("  member: {}", ws.selected_member_id);
        }
        let Some(plan) = resolved.compile_plan.as_ref() else {
            return;
        };

        println!("Build graph:");
        println!("  root: {}", plan.project_name);
        if plan.dependency_projects.is_empty() {
            println!("  deps: (none)");
        } else {
            for dependency in &plan.dependency_projects {
                println!(
                    "  root -> {} ({})",
                    dependency.dependency_name, dependency.project_name
                );
            }
        }

        if plan.has_std_dependency {
            println!("  corelib: project dependency detected");
        } else {
            println!("  corelib: none declared in project graph");
        }
    }

    /// Finish the spinner with `message`, or print a line in plain mode.
    pub fn finish_build(&self, message: impl Into<Cow<'static, str>>) {
        let msg = message.into().into_owned();
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message(msg);
        } else {
            eprintln!("{msg}");
        }
    }

    /// Whether this adapter is driving an indicatif spinner (TTY and not plain).
    pub fn is_spinner_enabled(&self) -> bool {
        self.spinner.is_some()
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
            eprintln!("  {msg}");
        } else if let Some(b) = &self.spinner {
            b.set_message(msg);
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
}

/// Resolve input with [`CliPipeline`] as the pipeline observer; returns UI handle and [`ResolvedInput`].
pub fn resolve_input_with_cli_pipeline(
    input: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: Option<&str>,
    workspace_member: Option<&str>,
    frozen: bool,
    locked: bool,
    plain: bool,
) -> Result<(Arc<CliPipeline>, ResolvedInput)> {
    let use_spinner = use_cli_spinner(plain);
    let pipeline_ui = Arc::new(CliPipeline::new(use_spinner));
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

/// Like [`resolve_input_with_cli_pipeline`], but resolves a full [`ResolvedProject`].
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
    let use_spinner = use_cli_spinner(plain);
    let pipeline_ui = Arc::new(CliPipeline::new(use_spinner));
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
        match event {
            PipelineEvent::PhaseStart { id } => {
                self.flush_pending_work_unit_ui();
                if let Ok(mut t) = self.work_unit_throttle.lock() {
                    t.reset_for_phase_boundary();
                }
                let msg = format!("{id} …");
                if self.plain {
                    eprintln!("→ {msg}");
                }
                if let Some(b) = &self.spinner {
                    b.set_message(msg);
                }
            }
            PipelineEvent::PhaseEnd { id } => {
                self.flush_pending_work_unit_ui();
                if let Ok(mut t) = self.work_unit_throttle.lock() {
                    t.reset_for_phase_boundary();
                }
                if self.plain {
                    eprintln!("✓ {id}");
                }
            }
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
