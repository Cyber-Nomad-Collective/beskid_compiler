//! Front-end type-check gates for selected `corelib_tests` entries.
//!
//! Run with progress visible:
//! `cargo test -p beskid_tests corelib_test -- --nocapture --test-threads=1`

use crate::projects::fixture_harness::{
    corelib_tests_project_root, typecheck_corelib_tests_entry, with_project_test_env,
};

macro_rules! corelib_typecheck_test {
    ($name:ident, $entry:literal) => {
        #[test]
        fn $name() {
            let path = corelib_tests_project_root().join("src").join($entry);
            if !path.is_file() {
                return;
            }
            with_project_test_env(&corelib_tests_project_root(), || {
                typecheck_corelib_tests_entry($entry);
            });
        }
    };
}

corelib_typecheck_test!(
    channel_api_tests_front_end_typechecks,
    "concurrency/ChannelApiTests.bd"
);
corelib_typecheck_test!(
    wait_group_tests_front_end_typechecks,
    "concurrency/WaitGroupTests.bd"
);
corelib_typecheck_test!(
    mutex_try_lock_tests_front_end_typechecks,
    "concurrency/MutexTryLockTests.bd"
);
corelib_typecheck_test!(
    controls_progress_bar_tests_front_end_typechecks,
    "console/ControlsProgressBarTests.bd"
);
corelib_typecheck_test!(core_results_tests_front_end_typechecks, "core/ResultsTests.bd");
