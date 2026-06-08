//! JIT regression for corelib `Console.Controls.Frame.Repeat` via project resolution.

use std::path::PathBuf;

use beskid_analysis::services::{PrepareMode, PrepareOptions, resolve_input};
use beskid_engine::Engine;
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_queries::{configure_db_for_project, prepare_compilation_with_db, with_db};

fn corelib_tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/beskid_corelib/tests/corelib_tests")
}

#[test]
fn jit_corelib_repeat_builds_string_entrypoint() {
    let project_root = corelib_tests_root();
    let entry = project_root.join("src/console/ControlsFrameTests.bd");
    configure_db_for_project(&project_root);
    let resolved = resolve_input(
        Some(&entry),
        Some(&project_root),
        Some("ConsoleControlsFrameTests"),
        None,
        false,
        false,
    )
    .expect("resolve corelib_tests ControlsFrameTests");
    let prepared = with_db(|db| {
        prepare_compilation_with_db(
            db,
            &resolved,
            PrepareOptions {
                mode: PrepareMode::Executable,
                ..Default::default()
            },
            None,
        )
    })
    .expect("prepare executable");
    let front = prepared.into_executable().expect("executable front-end");

    let mut engine = Engine::new();
    run_entrypoint_from_front_end_with_engine(
        &mut engine,
        &front,
        &resolved.source_path.display().to_string(),
        &resolved.source,
        "repeat_builds_string",
        None,
    )
    .expect("repeat_builds_string should pass");
}
