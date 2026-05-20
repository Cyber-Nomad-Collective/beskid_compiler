use std::sync::{Arc, Mutex};

use beskid_codegen::services::lower_source;
use beskid_engine::Engine;
use beskid_pipeline::{PipelineEvent, PipelineObserver, phases};

#[derive(Clone)]
struct Recorder(Arc<Mutex<Vec<&'static str>>>);

impl Recorder {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineObserver for Recorder {
    fn on_event(&self, event: PipelineEvent) {
        let mut g = self.0.lock().unwrap();
        match event {
            PipelineEvent::PhaseStart { id } => g.push(id),
            PipelineEvent::PhaseEnd { id } => g.push(id),
            PipelineEvent::WorkUnit { id, .. } => g.push(id),
        }
    }
}

#[test]
fn jit_compile_emits_emit_work_units_and_finalize_phase() {
    let src = r#"
pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false).unwrap();
    let recorder = Recorder::default();
    let obs: &dyn PipelineObserver = &recorder;

    let mut engine = Engine::new();
    engine
        .compile_artifact_with_pipeline(&lowered.artifact, Some(obs))
        .expect("compile");

    let events = recorder.0.lock().unwrap().clone();
    assert!(
        events.contains(&phases::JIT_EMIT),
        "expected jit.emit work units, got {events:?}"
    );
    assert!(
        events.contains(&phases::JIT_FINALIZE),
        "expected jit.finalize phase, got {events:?}"
    );
}
