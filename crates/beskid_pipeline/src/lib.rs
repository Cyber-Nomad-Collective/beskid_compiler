//! Shared compilation pipeline model: stable phase ids, events, and observers.
//!
//! This crate is **std-only**. UI (e.g. `indicatif`) lives in `beskid_cli` by implementing
//! [`PipelineObserver`]. Emitters (`beskid_analysis`, `beskid_codegen`, `beskid_aot`,
//! `beskid_engine`) depend on this leaf crate only.
//!
//! Canonical phase orders: [`phases::FULL_BUILD_PHASE_ORDER`] (host build),
//! [`phases::MOD_BUILD_PHASE_ORDER`] (mod artifact rebuild), [`phases::JIT_RUN_PHASE_ORDER`] (run/test).

pub mod grammar;
pub mod phases;
pub mod timing;

pub use grammar::GRAMMAR_REVISION;

use std::borrow::Cow;
use std::sync::Arc;

pub use phases::*;
pub use timing::TimedPipelineObserver;

/// A single pipeline observation (phase boundaries or fine-grained work units).
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    PhaseStart {
        id: &'static str,
    },
    PhaseEnd {
        id: &'static str,
    },
    WorkUnit {
        id: &'static str,
        done: u64,
        total: u64,
        label: Cow<'static, str>,
    },
}

/// Receives [`PipelineEvent`] from compiler stages. Implementations must be cheap; heavy work
/// (throttling, indicatif updates) belongs in the CLI adapter.
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
pub fn observe_phase<O: PipelineObserver + ?Sized>(
    obs: Option<&O>,
    id: &'static str,
    f: impl FnOnce(),
) {
    if let Some(o) = obs {
        o.on_event(PipelineEvent::PhaseStart { id });
        f();
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
    if let Some(o) = obs {
        o.on_event(PipelineEvent::PhaseStart { id });
        let out = f();
        o.on_event(PipelineEvent::PhaseEnd { id });
        out
    } else {
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
    if let Some(o) = obs {
        o.on_event(PipelineEvent::WorkUnit {
            id,
            done,
            total,
            label: label.into(),
        });
    }
}

/// Shared handle for stages that take owned requests (e.g. AOT build).
pub type SharedPipelineObserver = Arc<dyn PipelineObserver>;
