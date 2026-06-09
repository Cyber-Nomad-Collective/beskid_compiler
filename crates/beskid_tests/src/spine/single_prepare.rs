//! Conformance for single prepare on the `beskid run` path (Wave 1).
//!
//! Compile commands call [`beskid_queries::prepare_compilation_diagnostics`] once, then
//! [`lower_from_front_end`] on the returned front-end bundle.
//! One prepare spine must emit exactly one parent `semantic` and one parent `lower` phase.

use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

use beskid_analysis::services::{
    FrontEndOptions, PrepareOptions, ResolvedInput, resolve_input,
};
use beskid_codegen::services::lower_from_front_end;
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
        *self
            .starts
            .lock()
            .expect("phase recorder lock")
            .get(id)
            .unwrap_or(&0)
    }

    fn saw(&self, id: &'static str) -> bool {
        self.count(id) > 0
    }
}

impl PipelineObserver for PhaseStartRecorder {
    fn on_event(&self, event: PipelineEvent) {
        if let PipelineEvent::PhaseStart { id } = event {
            *self
                .starts
                .lock()
                .expect("phase recorder lock")
                .entry(id)
                .or_insert(0) += 1;
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

const LOWER_SUB_PHASE_IDS: &[&str] = &[
    phases::LOWER_AST,
    phases::LOWER_RESOLVE_PASS1,
    phases::LOWER_NORMALIZE,
    phases::LOWER_RESOLVE,
    phases::LOWER_TYPE_CHECK,
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

/// Mirrors Wave 1 `beskid run`: one executable prepare, then codegen lowering from cached front-end.
fn run_single_prepare_path(resolved: &ResolvedInput, observer: &PhaseStartRecorder) {
    let (prepared, _gate_diagnostics) = prepare_compilation_diagnostics(
        resolved,
        PrepareOptions {
            front_end: FrontEndOptions {
                with_semantic_diagnostics: true,
                ..Default::default()
            },
        },
        Some(observer),
    )
    .expect("executable gate prepare");

    let front = prepared.into_executable().expect("typed front-end");
    let source_name = resolved.source_path.display().to_string();
    lower_from_front_end(
        &source_name,
        &resolved.source,
        front,
        Some("Main"),
        Some(observer),
    )
    .expect("lower from prepared front-end");
}

/// Wave 1: one prepare spine per compile command on the run path.
#[test]
fn run_path_single_prepare_after_wave1() {
    let (root, entry) = minimal_run_fixture();

    with_cwd(&root, || {
        let resolved =
            resolve_input(Some(&entry), Some(&root), None, None, false, false).expect("resolve");
        configure_db_for_project(&root);

        let recorder = PhaseStartRecorder::default();
        run_single_prepare_path(&resolved, &recorder);

        assert_eq!(
            recorder.count(phases::PARSE),
            1,
            "Wave 1 run path must invoke prepare once (executable gate only)"
        );

        assert_eq!(
            recorder.count(phases::SEMANTIC),
            1,
            "Wave 1 run path must run semantic parent phase exactly once per command"
        );
        assert_eq!(
            recorder.count(phases::LOWER),
            1,
            "Wave 1 run path must run lower parent phase exactly once per command"
        );

        for id in SEMANTIC_SUB_PHASE_IDS {
            assert!(
                recorder.saw(id),
                "semantic sub-phase {id} should appear when observer is wired"
            );
        }
        for id in LOWER_SUB_PHASE_IDS {
            assert!(
                recorder.saw(id),
                "lower sub-phase {id} should appear when observer is wired"
            );
        }
    });

    let _ = fs::remove_dir_all(root);
}
