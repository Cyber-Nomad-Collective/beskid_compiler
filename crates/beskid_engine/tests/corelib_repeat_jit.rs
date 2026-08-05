//! JIT regression for corelib `Console.Controls.Frame.Repeat` via project resolution.

use std::path::PathBuf;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::services::{PrepareOptions, resolve_input};
use beskid_engine::Engine;
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_queries::{configure_db_for_project, prepare_compilation_with_db, with_db};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

fn corelib_tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/beskid_corelib/tests/corelib_tests")
}

#[test]
fn jit_corelib_repeat_builds_string_entrypoint() {
    let runtime_prefix = tempfile::tempdir().expect("exact runtime-kit prefix");
    build_native_host(runtime_prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish exact native runtime kit");
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

    let target = beskid_engine::host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(runtime_prefix.path(), target, BuildProfile::Debug)
        .expect("load exact native runtime kit");
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
