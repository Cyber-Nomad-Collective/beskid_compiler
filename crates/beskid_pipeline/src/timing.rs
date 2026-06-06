//! Wall-clock phase timing for pipeline observers.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{PipelineEvent, PipelineObserver};

/// Records per-phase wall time between [`PipelineEvent::PhaseStart`] and [`PipelineEvent::PhaseEnd`].
#[derive(Debug, Default)]
pub struct TimedPipelineObserver {
    active: Mutex<HashMap<&'static str, Instant>>,
    totals: Mutex<HashMap<&'static str, Duration>>,
}

impl TimedPipelineObserver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of accumulated phase durations (milliseconds).
    pub fn phase_millis(&self) -> HashMap<&'static str, u128> {
        self.totals
            .lock()
            .expect("timed pipeline totals")
            .iter()
            .map(|(id, duration)| (*id, duration.as_millis()))
            .collect()
    }

    /// Sum of selected phase durations.
    pub fn sum_millis(&self, phases: &[&'static str]) -> u128 {
        let totals = self.totals.lock().expect("timed pipeline totals");
        phases
            .iter()
            .filter_map(|id| totals.get(id))
            .map(|d| d.as_millis())
            .sum()
    }

    pub fn reset(&self) {
        self.active.lock().expect("timed pipeline active").clear();
        self.totals.lock().expect("timed pipeline totals").clear();
    }
}

impl PipelineObserver for TimedPipelineObserver {
    fn on_event(&self, event: PipelineEvent) {
        match event {
            PipelineEvent::PhaseStart { id } => {
                self.active
                    .lock()
                    .expect("timed pipeline active")
                    .insert(id, Instant::now());
            }
            PipelineEvent::PhaseEnd { id } => {
                let Some(started) = self
                    .active
                    .lock()
                    .expect("timed pipeline active")
                    .remove(&id)
                else {
                    return;
                };
                let elapsed = started.elapsed();
                self.totals
                    .lock()
                    .expect("timed pipeline totals")
                    .entry(id)
                    .and_modify(|total| *total += elapsed)
                    .or_insert(elapsed);
            }
            PipelineEvent::WorkUnit { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe_phase;

    #[test]
    fn timed_observer_accumulates_phase_duration() {
        let observer = TimedPipelineObserver::new();
        observe_phase(Some(&observer), "semantic", || {
            std::thread::sleep(Duration::from_millis(5));
        });
        let millis = observer
            .phase_millis()
            .get("semantic")
            .copied()
            .unwrap_or(0);
        assert!(millis >= 4, "expected at least 4ms, got {millis}");
    }
}
