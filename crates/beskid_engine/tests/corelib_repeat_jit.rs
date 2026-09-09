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

fn run_corelib_test(entry: &str, target_name: &str, test_name: &str) -> String {
    run_corelib_tests(entry, target_name, &[test_name])
        .into_iter()
        .next()
        .expect("one requested corelib entrypoint result")
}

fn run_corelib_tests(entry: &str, target_name: &str, test_names: &[&str]) -> Vec<String> {
    let runtime_prefix = tempfile::tempdir().expect("exact runtime-kit prefix");
    build_native_host(runtime_prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish exact native runtime kit");
    let project_root = corelib_tests_root();
    let entry = project_root.join(entry);
    configure_db_for_project(&project_root);
    let resolved = resolve_input(Some(&entry), Some(&project_root), Some(target_name), None, false, false)
        .expect("resolve corelib test target");
    let prepared =
        with_db(|db| prepare_compilation_with_db(db, &resolved, PrepareOptions { ..Default::default() }, None))
            .expect("prepare executable");
    let front = prepared.into_executable().expect("executable front-end");

    let target = beskid_engine::host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(runtime_prefix.path(), target, BuildProfile::Debug)
        .expect("load exact native runtime kit");
    test_names
        .iter()
        .map(|test_name| {
            run_entrypoint_from_front_end_with_engine(
                &mut engine,
                &front,
                &resolved.source_path.display().to_string(),
                &resolved.source,
                test_name,
                None,
            )
            .unwrap_or_else(|error| panic!("{test_name} should pass: {error}"))
        })
        .collect()
}

#[test]
fn jit_corelib_repeat_builds_string_entrypoint() {
    run_corelib_test("src/console/ControlsFrameTests.bd", "ConsoleControlsFrameTests", "repeat_builds_string");
}

#[test]
fn jit_progress_bar_mixed_interpolation_preserves_exact_fill() {
    run_corelib_test(
        "src/console/ControlsProgressBarTests.bd",
        "ConsoleControlsProgressBarTests",
        "progress_bar_renders_body_with_exact_fill",
    );
}

#[test]
fn jit_numeric_interpolation_preserves_value() {
    run_corelib_test(
        "src/console/ControlsProgressBarTests.bd",
        "ConsoleControlsProgressBarTests",
        "progress_bar_formats_numeric_interpolation",
    );
}

#[test]
fn jit_numeric_interpolation_has_decimal_bytes() {
    let entry = "src/console/ControlsProgressBarTests.bd";
    let target = "ConsoleControlsProgressBarTests";
    let actual = run_corelib_tests(
        entry,
        target,
        &["NumericInterpolationLength", "NumericInterpolationFirstByte", "NumericInterpolationSecondByte"],
    );
    assert_eq!(actual, ["2", "53", "48"]);
}

#[test]
fn jit_adjacent_string_interpolation_preserves_values() {
    run_corelib_test(
        "src/console/ControlsProgressBarTests.bd",
        "ConsoleControlsProgressBarTests",
        "progress_bar_concatenates_adjacent_string_interpolations",
    );
}
