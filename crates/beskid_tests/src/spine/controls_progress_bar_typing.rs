//! `use Console.Controls.ProgressBar; ProgressBar.ProgressBar.*` must resolve in corelib tests.

use crate::projects::fixture_harness::{
    corelib_tests_project_root, typecheck_corelib_tests_entry, with_project_test_env,
};

#[test]
fn controls_progress_bar_tests_front_end_typechecks() {
    let entry = corelib_tests_project_root().join("src/console/ControlsProgressBarTests.bd");
    if !entry.is_file() {
        return;
    }
    with_project_test_env(&corelib_tests_project_root(), || {
        typecheck_corelib_tests_entry("console/ControlsProgressBarTests.bd");
    });
}
