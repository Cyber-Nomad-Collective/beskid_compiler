use std::fs;
use std::sync::Mutex;

use beskid_analysis::mod_host::{
    ModHostInput, run_analyze_rewrite_with_invoker, run_through_generate,
};
use beskid_analysis::projects::{
    CompilePlan, ResolvedDependencyProject, Target, TargetKind,
};
use beskid_analysis::services::SemanticSnapshot;
use beskid_analysis::services::parse_program_with_source_name;
use beskid_pipeline::phases::{
    COMPOSITION_RESOLVE, FULL_BUILD_PHASE_ORDER, MACRO_EXPAND, MOD_ANALYZE, MOD_COLLECT,
    MOD_GENERATE, MOD_LOAD, MOD_REWRITE, SEMANTIC_SNAPSHOT,
};
use beskid_pipeline::{PipelineEvent, PipelineObserver};

use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

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
fn descriptor_backed_noop_mod_emits_mod_phases_in_order() {
    let root = temp_case_dir("mod_pipeline_order");
    let host_dir = root.join("Host");
    let mod_dir = root.join("ModA");
    fs::create_dir_all(host_dir.join("Src")).expect("host source root");
    fs::create_dir_all(mod_dir.join("Src")).expect("mod source root");
    fs::write(host_dir.join("Src/Main.bd"), "unit main() { return; }\n").expect("host source");
    fs::write(mod_dir.join("Src/Mod.bd"), "unit marker() { return; }\n").expect("mod source");

    write_manifest(
        &host_dir,
        r#"
project {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "ModA" {
  source = path
  path = "../ModA"
}
"#,
    );
    write_manifest(
        &mod_dir,
        r#"
project {
  name = "ModA"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot, rewrite_syntax]
  }
}
"#,
    );
    write_noop_descriptor(&host_dir, "ModA");

    let source = "unit main() { return; }\n";
    let program = parse_program_with_source_name("Main.bd", source).expect("parse host program");
    let plan = CompilePlan {
        project_root: host_dir.clone(),
        manifest_path: host_dir.join("Host.bproj"),
        project_name: "Host".to_string(),
        source_root: host_dir.join("Src"),
        target: Target {
            name: "main".to_string(),
            kind: TargetKind::App,
            entry: Some("Main.bd".to_string()),
        },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "ModA".to_string(),
            manifest_path: mod_dir.join("ModA.bproj"),
            project_root: mod_dir.clone(),
            project_name: "ModA".to_string(),
            source_root: mod_dir.join("Src"),
        }],
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let pipeline = CapturePipeline::default();

    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: Some(&pipeline),
            invoker: None,
        },
    )
    .expect("mod host generate");
    assert_eq!(generated.session.loaded_descriptor_count(), 1);

    let composition_snapshot = generated.session.composition_snapshot_or_default();
    let semantic_snapshot = SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
        .with_composition(&composition_snapshot);

    let _program = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        None,
        Some(&semantic_snapshot),
        Some(&pipeline),
    )
    .expect("mod host analyze/rewrite")
    .program;

    let events = pipeline.phase_starts();
    assert_subsequence(
        &events,
        &[
            MACRO_EXPAND,
            MOD_LOAD,
            MOD_COLLECT,
            MOD_GENERATE,
            MACRO_EXPAND,
            MOD_ANALYZE,
            MOD_REWRITE,
        ],
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn full_build_phase_order_keeps_mod_hooks_between_parse_and_lower_ready() {
    assert_subsequence(
        FULL_BUILD_PHASE_ORDER,
        &[
            MACRO_EXPAND,
            MOD_LOAD,
            MOD_COLLECT,
            MOD_GENERATE,
            SEMANTIC_SNAPSHOT,
            COMPOSITION_RESOLVE,
            MOD_ANALYZE,
            MOD_REWRITE,
        ],
    );
}

fn write_noop_descriptor(host_dir: &std::path::Path, package_id: &str) {
    let descriptor_dir = host_dir
        .join(".beskid")
        .join("obj")
        .join("mods")
        .join(package_id)
        .join("cache-key")
        .join("test-triple");
    fs::create_dir_all(&descriptor_dir).expect("descriptor cache dir");
    fs::write(
        descriptor_dir.join("mod.descriptor.json"),
        r#"{
  "schemaVersion": 1,
  "packageId": "ModA",
  "modSourceHash": "source",
  "lockHash": "lock",
  "targetTriple": "test-triple",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": [
    {
      "contractId": "Beskid.Compiler.Collect.Collector",
      "typeId": "ModA.Collect",
      "entrySymbol": "moda_collect"
    },
    {
      "contractId": "Beskid.Compiler.Collect.Generator",
      "typeId": "ModA.Generate",
      "entrySymbol": "moda_generate"
    },
    {
      "contractId": "Beskid.Compiler.Collect.Analyzer",
      "typeId": "ModA.Analyze",
      "entrySymbol": "moda_analyze"
    },
    {
      "contractId": "Beskid.Compiler.Collect.Rewriter",
      "typeId": "ModA.Rewrite",
      "entrySymbol": "moda_rewrite"
    }
  ]
}"#,
    )
    .expect("descriptor");
}

fn assert_subsequence(events: &[&'static str], expected: &[&'static str]) {
    let mut cursor = 0usize;
    for event in events {
        if cursor < expected.len() && *event == expected[cursor] {
            cursor += 1;
        }
    }
    assert_eq!(
        cursor,
        expected.len(),
        "expected phase subsequence {expected:?} in observed events {events:?}"
    );
}
