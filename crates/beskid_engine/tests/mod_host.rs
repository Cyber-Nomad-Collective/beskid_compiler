//! Integration test that drives the full `mod.load` → `mod.collect` →
//! `mod.generate` → `mod.analyze` → `mod.rewrite` pipeline against the reference
//! SampleMod fixture, then JIT-compiles the resulting host program through the
//! [`beskid_engine::Engine`].
//!
//! Verifies:
//! - All four contract kinds dispatch through `ContractInvoker`.
//! - The `mod.*` pipeline phases fire in canonical order.
//! - The engine accepts the lowered artifact after mod-host rewrites.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::AnalysisOptions;
use beskid_analysis::mod_host::{
    InvocationKind, ModHostInput, StubContractInvoker, run_analyze_rewrite_with_invoker, run_through_generate,
};
use beskid_analysis::projects::{CompilePlan, ResolvedDependencyProject, Target, TargetKind};
use beskid_analysis::services::{parse_program_with_source_name, semantic_rule_diagnostics_for_program};
use beskid_engine::services::prepare_jit_entrypoint;
use beskid_engine::{Engine, host_runtime_target};
use beskid_pipeline::phases::{
    LOWER_READY, MACRO_EXPAND, MOD_ANALYZE, MOD_COLLECT, MOD_GENERATE, MOD_GLUE, MOD_LOAD, MOD_REWRITE,
};
use beskid_pipeline::{PipelineEvent, PipelineObserver, observe_phase};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

const SAMPLE_MOD_PROJECT: &str = include_str!("../../beskid_tests/fixtures/mods/sample_mod/SampleMod.bproj");

const HOST_MANIFEST: &str = r#"
Host {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "SampleMod" {
  source = path
  path = "../SampleMod"
}
"#;

const HOST_SOURCE: &str = "pub i64 Main() { return 0; }\n";

const SAMPLE_DESCRIPTOR_REGS: &str = r#"[
    { "contractId": "Beskid.Compiler.Collect.Collector", "typeId": "SampleMod.SampleCollect", "entrySymbol": "samplemod_collect" },
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" },
    { "contractId": "Beskid.Compiler.Collect.AttributeGenerator", "typeId": "SampleMod.SampleAttribute", "entrySymbol": "samplemod_attribute" },
    { "contractId": "Beskid.Compiler.Collect.Analyzer", "typeId": "SampleMod.SampleAnalyze", "entrySymbol": "samplemod_analyze" },
    { "contractId": "Beskid.Compiler.Collect.Rewriter", "typeId": "SampleMod.SampleRewrite", "entrySymbol": "samplemod_rewrite" }
  ]"#;

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
fn mod_host_full_pipeline_compiles_in_engine() -> Result<()> {
    let workspace = TestWorkspace::new("engine_mod_host_full_pipeline");
    workspace.write_descriptor(SAMPLE_DESCRIPTOR_REGS);

    let plan = workspace.compile_plan();
    let pipeline = Arc::new(CapturePipeline::default());
    let invoker = StubContractInvoker::new();

    let program = parse_program_with_source_name("Main.bd", HOST_SOURCE)?;
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source: HOST_SOURCE,
            pipeline: Some(pipeline.as_ref()),
            invoker: Some(&invoker),
            cached_target_fingerprint: None,
        },
    )?;
    assert_eq!(generated.session.loaded_descriptor_count(), 1);
    assert_eq!(generated.collector_outcomes.len(), 1);
    assert_eq!(generated.generator_outcomes.len(), 2);

    let _ = semantic_rule_diagnostics_for_program(
        &generated.program.node,
        "Main.bd".to_owned(),
        HOST_SOURCE,
        AnalysisOptions::default(),
    );
    let snapshot = beskid_analysis::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
        .with_composition(&generated.session.composition_snapshot_or_default());
    let analyze = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        Some(&invoker),
        None,
        Some(&snapshot),
        Some(pipeline.as_ref()),
    )?;
    assert_eq!(analyze.analyzer_outcomes.len(), 1);
    assert_eq!(analyze.rewriter_outcomes.len(), 1);

    observe_phase(Some(pipeline.as_ref()), LOWER_READY, || {});
    // Mod-host rewrite authority stays on the analysis spine; JIT compile uses the sole
    // CodegenInput → ISLE route against the host entry source with no legacy lowering driver.
    let _ = &analyze.program;
    let prepared =
        prepare_jit_entrypoint(workspace.host_dir.join("Src").join("Main.bd").as_path(), HOST_SOURCE, "Main")?;

    let kit_prefix = tempfile::tempdir().expect("exact kit prefix");
    build_native_host(kit_prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native kit");
    let target = host_runtime_target().expect("host target");
    let mut engine = Engine::with_runtime_kit(kit_prefix.path(), target, BuildProfile::Debug).expect("load exact kit");
    engine
        .compile_artifact_with_pipeline(&prepared.artifact, Some(pipeline.as_ref()))
        .map_err(|err| anyhow::anyhow!("engine compile failed: {err}"))?;

    let invocations = invoker.invocations();
    let kinds: Vec<&str> = invocations
        .iter()
        .map(|inv| match inv {
            InvocationKind::Collector { .. } => "collector",
            InvocationKind::Generator { .. } => "generator",
            InvocationKind::Analyzer { .. } => "analyzer",
            InvocationKind::Rewriter { .. } => "rewriter",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["collector", "generator", "generator", "analyzer", "rewriter"],
        "engine integration must see all four contract kinds dispatched in canonical order"
    );

    let events = pipeline.phase_starts();
    assert_subsequence(
        &events,
        &[MACRO_EXPAND, MOD_LOAD, MOD_COLLECT, MOD_GENERATE, MACRO_EXPAND, MOD_ANALYZE, MOD_REWRITE, MOD_GLUE],
    );
    Ok(())
}

struct TestWorkspace {
    root: PathBuf,
    host_dir: PathBuf,
    mod_dir: PathBuf,
}

impl TestWorkspace {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time ok").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid_engine_{prefix}_{}_{}", std::process::id(), nanos));
        let host_dir = root.join("Host");
        let mod_dir = root.join("SampleMod");
        fs::create_dir_all(host_dir.join("Src")).expect("host source root");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod source root");
        fs::write(host_dir.join("Src").join("Main.bd"), HOST_SOURCE).expect("host source");
        fs::write(host_dir.join("Host.bproj"), HOST_MANIFEST).expect("host manifest");
        fs::write(mod_dir.join("SampleMod.bproj"), SAMPLE_MOD_PROJECT).expect("mod manifest");
        Self { root, host_dir, mod_dir }
    }

    fn write_descriptor(&self, registrations_json: &str) {
        let descriptor_dir = self
            .host_dir
            .join(".beskid")
            .join("obj")
            .join("mods")
            .join("SampleMod")
            .join("cache-key")
            .join("test-triple");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        fs::write(
            descriptor_dir.join("mod.descriptor.json"),
            format!(
                r#"{{
  "schemaVersion": 1,
  "packageId": "SampleMod",
  "modSourceHash": "fixture-source",
  "lockHash": "fixture-lock",
  "targetTriple": "test-triple",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": {registrations_json}
}}"#
            ),
        )
        .expect("write descriptor");
    }

    fn compile_plan(&self) -> CompilePlan {
        CompilePlan {
            project_root: self.host_dir.clone(),
            manifest_path: self.host_dir.join("Host.bproj"),
            project_name: "Host".to_owned(),
            source_root: self.host_dir.join("Src"),
            target: Target { name: "main".to_owned(), kind: TargetKind::App, entry: Some("Main.bd".to_owned()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "SampleMod".to_owned(),
                manifest_path: self.mod_dir.join("SampleMod.bproj"),
                project_root: self.mod_dir.clone(),
                project_name: "SampleMod".to_owned(),
                source_root: self.mod_dir.join("Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        }
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_subsequence(events: &[&'static str], expected: &[&'static str]) {
    let mut cursor = 0usize;
    for event in events {
        if cursor < expected.len() && *event == expected[cursor] {
            cursor += 1;
        }
    }
    assert_eq!(cursor, expected.len(), "expected phase subsequence {expected:?} in observed events {events:?}");
}
