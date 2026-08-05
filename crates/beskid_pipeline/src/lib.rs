//! Shared compilation pipeline model: stable phase ids, events, and observers.
//!
//! This crate is **std-only**. UI (e.g. Ratatui) lives in `beskid_tools` by implementing
//! [`PipelineObserver`]. Emitters (`beskid_analysis`, `beskid_codegen`, `beskid_aot`,
//! `beskid_engine`) depend on this leaf crate only.
//!
//! Canonical phase orders: [`phases::FULL_BUILD_PHASE_ORDER`] (host build),
//! [`phases::MOD_BUILD_PHASE_ORDER`] (mod artifact rebuild), [`phases::JIT_RUN_PHASE_ORDER`]
//! (interim run/test JIT), [`phases::RUN_AOT_PHASE_ORDER`] (AOT subprocess run).

pub mod grammar;
pub mod phases;
pub mod timing;

pub use grammar::GRAMMAR_REVISION;

use std::borrow::Cow;
use std::sync::Arc;

pub use phases::*;
pub use timing::TimedPipelineObserver;

/// Floor for workers that execute compiler phases which traverse the complete semantic graph.
///
/// This policy belongs in the pipeline leaf crate so command entrypoints and internal worker
/// pools cannot silently diverge. The environment may request a larger stack, never a smaller
/// one.
pub const COMPILER_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Resolve the compiler worker stack size, honoring `RUST_MIN_STACK` only above the floor.
pub fn compiler_stack_size() -> usize {
    resolve_compiler_stack_size(std::env::var("RUST_MIN_STACK").ok().as_deref())
}

fn resolve_compiler_stack_size(requested: Option<&str>) -> usize {
    requested
        .and_then(|value| value.trim().parse::<usize>().ok())
        .map_or(COMPILER_STACK_SIZE, |size| size.max(COMPILER_STACK_SIZE))
}

/// A single pipeline observation (phase boundaries or fine-grained work units).
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    PhaseStart { id: &'static str },
    PhaseEnd { id: &'static str },
    WorkUnit { id: &'static str, done: u64, total: u64, label: Cow<'static, str> },
}

/// Receives [`PipelineEvent`] from compiler stages. Implementations must be cheap; heavy work
/// (throttling, progress bar updates) belongs in the CLI adapter.
pub trait PipelineObserver: Send + Sync {
    fn on_event(&self, event: PipelineEvent);
}

/// No-op observer for `Option<&dyn PipelineObserver>` call sites.
#[derive(Debug, Default, Copy, Clone)]
pub struct NoopPipeline;

impl PipelineObserver for NoopPipeline {
    fn on_event(&self, _event: PipelineEvent) {}
}

/// Runs `f` wrapped in [`PipelineEvent::PhaseStart`] / [`PipelineEvent::PhaseEnd`] when `obs` is present.
pub fn observe_phase<O: PipelineObserver + ?Sized>(obs: Option<&O>, id: &'static str, f: impl FnOnce()) {
    let span = tracing::info_span!(target: "beskid.pipeline", "pipeline.phase", phase = id);
    let _guard = span.enter();
    if let Some(o) = obs {
        tracing::debug!(target: "beskid.pipeline", "phase start");
        o.on_event(PipelineEvent::PhaseStart { id });
        f();
        tracing::debug!(target: "beskid.pipeline", "phase end");
        o.on_event(PipelineEvent::PhaseEnd { id });
    } else {
        f();
    }
}

/// Like [`observe_phase`], but `f` returns `Result` and phase end is still emitted on error paths.
pub fn observe_phase_result<T, E, O: PipelineObserver + ?Sized>(
    obs: Option<&O>,
    id: &'static str,
    f: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let span = tracing::info_span!(target: "beskid.pipeline", "pipeline.phase", phase = id);
    let _guard = span.enter();
    if let Some(o) = obs {
        tracing::debug!(target: "beskid.pipeline", "phase start");
        o.on_event(PipelineEvent::PhaseStart { id });
        let out = f();
        tracing::debug!(target: "beskid.pipeline", "phase end");
        o.on_event(PipelineEvent::PhaseEnd { id });
        out
    } else {
        f()
    }
}

/// Like [`observe_phase`], but preserves the value returned by `f`.
pub fn observe_phase_value<T, O: PipelineObserver + ?Sized>(
    obs: Option<&O>,
    id: &'static str,
    f: impl FnOnce() -> T,
) -> T {
    if let Some(o) = obs {
        observe_phase_result(Some(o), id, || Ok::<T, std::convert::Infallible>(f())).unwrap()
    } else {
        let span = tracing::info_span!(target: "beskid.pipeline", "pipeline.phase", phase = id);
        let _guard = span.enter();
        f()
    }
}

/// Emit a [`PipelineEvent::WorkUnit`] when `obs` is present.
pub fn emit_work_unit<O: PipelineObserver + ?Sized>(
    obs: Option<&O>,
    id: &'static str,
    done: u64,
    total: u64,
    label: impl Into<Cow<'static, str>>,
) {
    let label = label.into();
    tracing::trace!(
        target: "beskid.pipeline",
        phase = id,
        done,
        total,
        label = label.as_ref(),
        "work unit"
    );
    if let Some(o) = obs {
        o.on_event(PipelineEvent::WorkUnit { id, done, total, label });
    }
}

/// Log the current unit to tracing and forward a [`PipelineEvent::WorkUnit`] to observers.
pub fn report_progress<O: PipelineObserver + ?Sized>(
    obs: Option<&O>,
    id: &'static str,
    done: u64,
    total: u64,
    label: impl Into<Cow<'static, str>>,
) {
    let label = label.into();
    if id.starts_with("semantic") {
        tracing::info!(
            target: "beskid_tools::pipeline::semantic",
            phase = id,
            done,
            total,
            label = label.as_ref(),
            "progress"
        );
    } else {
        tracing::info!(
            target: "beskid_tools::pipeline::build",
            phase = id,
            done,
            total,
            label = label.as_ref(),
            "progress"
        );
    }
    emit_work_unit(obs, id, done, total, label);
}

/// Shared handle for stages that take owned requests (e.g. AOT build).
pub type SharedPipelineObserver = Arc<dyn PipelineObserver>;

#[cfg(test)]
mod tests {
    use super::{COMPILER_STACK_SIZE, resolve_compiler_stack_size};

    #[test]
    fn compiler_stack_policy_never_uses_a_smaller_requested_stack() {
        assert_eq!(resolve_compiler_stack_size(None), COMPILER_STACK_SIZE);
        assert_eq!(resolve_compiler_stack_size(Some("1048576")), COMPILER_STACK_SIZE);
        assert_eq!(resolve_compiler_stack_size(Some("0")), COMPILER_STACK_SIZE);
        assert_eq!(resolve_compiler_stack_size(Some("invalid")), COMPILER_STACK_SIZE);
    }

    #[test]
    fn compiler_stack_policy_accepts_a_larger_requested_stack() {
        let requested = COMPILER_STACK_SIZE * 2;
        assert_eq!(resolve_compiler_stack_size(Some(&format!(" {requested} "))), requested);
    }
}
