use std::path::Path;
use std::sync::{Arc, Mutex};

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::prepare_jit_entrypoint;
use beskid_engine::{Engine, host_runtime_target};
use beskid_pipeline::{PipelineEvent, PipelineObserver, phases};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

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
    let prefix = tempfile::tempdir().expect("exact kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish exact native kit");
    let target = host_runtime_target().expect("host target");
    let mut engine = Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug)
        .expect("load exact kit");

    let src = r#"
pub i64 Main() { return 0; }
"#;
    let prepared = prepare_jit_entrypoint(Path::new("<memory>"), src, "Main").unwrap();
    let recorder = Recorder::default();
    let obs: &dyn PipelineObserver = &recorder;

    engine
        .compile_artifact_with_pipeline(&prepared.artifact, Some(obs))
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
