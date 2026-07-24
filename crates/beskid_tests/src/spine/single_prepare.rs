//! Conformance for single prepare on the `beskid run` path (Wave 1).
//!
//! Compile commands call [`beskid_queries::prepare_compilation_diagnostics`] once, then
//! generation-safe syntax lowering on the returned front-end bundle.
//! One prepare spine must emit exactly one parent `semantic` phase.

use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

use beskid_analysis::services::{FrontEndOptions, PrepareOptions, ResolvedInput, resolve_input};
use beskid_pipeline::{PipelineEvent, PipelineObserver, phases};
use beskid_queries::{configure_db_for_project, prepare_compilation_diagnostics};

use crate::projects::with_cwd;
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

/// Records [`PipelineEvent::PhaseStart`] counts keyed by stable phase id.
#[derive(Default)]
struct PhaseStartRecorder {
    starts: Mutex<HashMap<&'static str, usize>>,
}

impl PhaseStartRecorder {
    fn count(&self, id: &'static str) -> usize {
        *self.starts.lock().expect("phase recorder lock").get(id).unwrap_or(&0)
    }

    fn saw(&self, id: &'static str) -> bool {
        self.count(id) > 0
    }
}

impl PipelineObserver for PhaseStartRecorder {
    fn on_event(&self, event: PipelineEvent) {
        if let PipelineEvent::PhaseStart { id } = event {
            *self.starts.lock().expect("phase recorder lock").entry(id).or_insert(0) += 1;
        }
    }
}

const SEMANTIC_SUB_PHASE_IDS: &[&str] = &[
    phases::SEMANTIC_AST_LOWER,
    phases::SEMANTIC_DEFINITIONS,
    phases::SEMANTIC_CONTROL_FLOW,
    phases::SEMANTIC_NAME_RESOLUTION,
    phases::SEMANTIC_VISIBILITY,
    phases::SEMANTIC_CONTRACTS,
    phases::SEMANTIC_ERROR_HANDLING,
    phases::SEMANTIC_TYPE_CHECK,
    phases::SEMANTIC_NAMING_STYLE,
];

fn minimal_run_fixture() -> (std::path::PathBuf, std::path::PathBuf) {
    let root = temp_case_dir("spine_single_prepare");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "PrepareSmoke"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );
    let entry = src_dir.join("Main.bd");
    fs::write(
        &entry,
        r#"
i32 Main() {
    return 1;
}
"#,
    )
    .expect("write entry");
    (root, entry)
}

/// Mirrors Wave 1 `beskid run`: one executable prepare, then syntax lowering from cached front-end.
fn run_single_prepare_path(resolved: &ResolvedInput, observer: &PhaseStartRecorder) {
    let (prepared, _gate_diagnostics) = prepare_compilation_diagnostics(
        resolved,
        PrepareOptions {
            front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
            ..Default::default()
        },
        Some(observer),
    )
    .expect("executable gate prepare");

    let front = prepared.into_executable().expect("typed front-end");
    beskid_engine::services::lower_prepared_syntax_entrypoint(
        &front,
        "Main",
        beskid_engine::host_runtime_target().expect("host ABI-v5 target"),
    )
    .expect("lower from prepared syntax front-end");
}

/// Wave 1: one prepare spine per compile command on the run path.
#[test]
fn run_path_single_prepare_after_wave1() {
    let (root, entry) = minimal_run_fixture();

    with_cwd(&root, || {
        let resolved = resolve_input(Some(&entry), Some(&root), None, None, false, false).expect("resolve");
        configure_db_for_project(&root);

        let recorder = PhaseStartRecorder::default();
        run_single_prepare_path(&resolved, &recorder);

        assert_eq!(recorder.count(phases::PARSE), 1, "Wave 1 run path must invoke prepare once (executable gate only)");

        assert_eq!(
            recorder.count(phases::SEMANTIC),
            1,
            "Wave 1 run path must run semantic parent phase exactly once per command"
        );
        for id in SEMANTIC_SUB_PHASE_IDS {
            assert!(recorder.saw(id), "semantic sub-phase {id} should appear when observer is wired");
        }
    });

    let _ = fs::remove_dir_all(root);
}

fn orphan_bd_fixture() -> (std::path::PathBuf, std::path::PathBuf) {
    let root = temp_case_dir("spine_orphan_bd");
    let entry = root.join("Orphan.bd");
    fs::write(
        &entry,
        r#"
i32 Main() {
    return 1;
}
"#,
    )
    .expect("write orphan entry");
    (root, entry)
}

/// Orphan `.bd` without `.bproj` uses a synthetic compile plan on the CLI resolve path.
#[test]
fn orphan_bd_single_prepare_without_bproj() {
    let (root, entry) = orphan_bd_fixture();

    with_cwd(&root, || {
        let resolved = resolve_input(Some(&entry), None, None, None, false, false).expect("resolve orphan");

        let plan = resolved.compile_plan.as_ref().expect("orphan .bd should get synthetic compile plan");
        assert_eq!(plan.project_name, "__synthetic__");
        assert_eq!(plan.target.entry.as_deref(), Some("Orphan.bd"));

        let recorder = PhaseStartRecorder::default();
        run_single_prepare_path(&resolved, &recorder);

        assert_eq!(recorder.count(phases::PARSE), 1, "orphan .bd run path must invoke prepare once");
        assert_eq!(recorder.count(phases::SEMANTIC), 1);
        assert_eq!(recorder.count(phases::LOWER), 1);
    });

    let _ = fs::remove_dir_all(root);
}
