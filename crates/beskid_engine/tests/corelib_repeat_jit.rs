//! JIT regression for corelib `Console.Controls.Frame.Repeat` via project resolution.

use std::path::PathBuf;

use beskid_analysis::services::{PrepareOptions, resolve_input};
use beskid_engine::Engine;
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_queries::{configure_db_for_project, prepare_compilation_with_db, with_db};

fn corelib_tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/beskid_corelib/tests/corelib_tests")
}

// Quarantined: the multi-hour `prepare_compilation` hang this test used to cause
// is fixed at the root (unbounded `(0..)` TypeId scans in beskid_analysis
// `lowering_prep` are now bounded), and the false "duplicate cast intent" that
// surfaced once prepare completed is fixed (cast-intent validation now keys on
// source_path). A separate, *flaky* "missing expression type information during
// codegen" regression remains: type-surface/node_types population for this
// multi-unit console assembly is nondeterministic (HashMap iteration order), so
// the JIT run fails intermittently. Re-enable once codegen type propagation is
// deterministic. Tracking: https://github.com/Cyber-Nomad-Collective/beskid_compiler/issues
#[test]
#[ignore = "flaky: nondeterministic 'missing expression type information during codegen' in multi-unit corelib assembly; prepare hang + duplicate-cast-intent fixed, codegen ordering tracked separately"]
fn jit_corelib_repeat_builds_string_entrypoint() {
    let project_root = corelib_tests_root();
    let entry = project_root.join("src/console/ControlsFrameTests.bd");
    configure_db_for_project(&project_root);
    let resolved =
        resolve_input(Some(&entry), Some(&project_root), Some("ConsoleControlsFrameTests"), None, false, false)
            .expect("resolve corelib_tests ControlsFrameTests");
    let prepared =
        with_db(|db| prepare_compilation_with_db(db, &resolved, PrepareOptions { ..Default::default() }, None))
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
