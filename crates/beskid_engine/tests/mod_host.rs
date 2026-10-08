//! Integration test that JIT-compiles a host program through the [`beskid_engine::Engine`] after
//! the `mod.*` pipeline phases.
//!
//! Native Mod execution requires a qualified executable Mod artifact (executable descriptor V2,
//! exact runtime kit, built shared library); fixtures cannot forge one, so a descriptor-less host
//! with no Mod dependencies is driven here. Mod scheduling, dispatch order and registration
//! conflicts are covered by `beskid_analysis` `mod_host::api::scheduling_tests`, and real
//! executable qualification by the CLI native Mod tests.
//!
//! Verifies:
//! - The mod host is a no-op that only expands macros when the plan has no Mod dependencies.
//! - The engine accepts the lowered artifact after the mod host ran.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::mod_host::{ModHostInput, run_analyze_rewrite_with_invoker, run_through_generate};
use beskid_analysis::projects::{CompilePlan, Target, TargetKind};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_engine::services::prepare_jit_entrypoint;
use beskid_engine::{Engine, host_runtime_target};
use beskid_pipeline::phases::{LOWER_READY, MACRO_EXPAND, MOD_LOAD};
use beskid_pipeline::{PipelineEvent, PipelineObserver, observe_phase};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

const HOST_MANIFEST: &str = r#"
Host {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#;

const HOST_SOURCE: &str = "pub i64 Main() { return 0; }\n";

#[derive(Default)]
struct CapturePipeline {
    phase_starts: Mutex<Vec<&'static str>>,
}

impl CapturePipeline {
    fn phase_starts(&self) -> Vec<&'static str> {
        self.phase_starts.lock().expect("phase starts").clone()
    }
}

impl PipelineObserver for CapturePipeline {
    fn on_event(&self, event: PipelineEvent) {
        if let PipelineEvent::PhaseStart { id } = event {
            self.phase_starts.lock().expect("phase starts").push(id);
        }
    }
}

#[test]
fn mod_host_without_mod_dependencies_compiles_in_engine() -> Result<()> {
    let workspace = TestWorkspace::new("engine_mod_host_no_mods");
    let plan = workspace.compile_plan();
    let pipeline = Arc::new(CapturePipeline::default());

    let program = parse_program_with_source_name("Main.bd", HOST_SOURCE)?;
    let generated = run_through_generate(
        program,
        &ModHostInput {
            semantic_scope: None,
            semantic_authority: None,
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source: HOST_SOURCE,
            pipeline: Some(pipeline.as_ref()),
            invoker: None,
            cached_target_fingerprint: None,
            syntax_generation_id: None,
        },
    )?;
    assert!(generated.session.is_empty());
    assert!(generated.collector_outcomes.is_empty());
    assert!(generated.generator_outcomes.is_empty());

    let snapshot = beskid_analysis::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
        .with_composition(&generated.session.composition_snapshot_or_default());
    let analyze = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        None,
        None,
        Some(&snapshot),
        Some(pipeline.as_ref()),
    )?;
    assert!(analyze.analyzer_outcomes.is_empty());
    assert!(analyze.rewriter_outcomes.is_empty());

    observe_phase(Some(pipeline.as_ref()), LOWER_READY, || {});
    // JIT compile uses the sole CodegenInput -> ISLE route against the host entry source.
    let prepared =
        prepare_jit_entrypoint(workspace.host_dir.join("Src").join("Main.bd").as_path(), HOST_SOURCE, "Main")?;

    let kit_prefix = tempfile::tempdir().expect("exact kit prefix");
    build_native_host(kit_prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native kit");
    let target = host_runtime_target().expect("host target");
    let mut engine = Engine::with_runtime_kit(kit_prefix.path(), target, BuildProfile::Debug).expect("load exact kit");
    engine
        .compile_artifact_with_pipeline(&prepared.artifact, Some(pipeline.as_ref()))
        .map_err(|err| anyhow::anyhow!("engine compile failed: {err}"))?;

    let events = pipeline.phase_starts();
    assert!(events.contains(&MACRO_EXPAND), "macro expansion always runs: {events:?}");
    assert!(!events.contains(&MOD_LOAD), "no Mod dependency means no mod.load phase: {events:?}");
    Ok(())
}

struct TestWorkspace {
    root: PathBuf,
    host_dir: PathBuf,
}

impl TestWorkspace {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time ok").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid_engine_{prefix}_{}_{}", std::process::id(), nanos));
        let host_dir = root.join("Host");
        fs::create_dir_all(host_dir.join("Src")).expect("host source root");
        fs::write(host_dir.join("Src").join("Main.bd"), HOST_SOURCE).expect("host source");
        fs::write(host_dir.join("Host.bproj"), HOST_MANIFEST).expect("host manifest");
        Self { root, host_dir }
    }

    fn compile_plan(&self) -> CompilePlan {
        CompilePlan {
            project_root: self.host_dir.clone(),
            manifest_path: self.host_dir.join("Host.bproj"),
            project_name: "Host".to_owned(),
            source_root: self.host_dir.join("Src"),
            target: Target { name: "main".to_owned(), kind: TargetKind::App, entry: Some("Main.bd".to_owned()) },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_core_dependency: false,
        }
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
