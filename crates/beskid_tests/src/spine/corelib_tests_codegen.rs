//! CLIF lowering gates for selected `corelib_tests` entries (per-test link plan).

use crate::projects::fixture_harness::{
    corelib_tests_project_root, lower_corelib_tests_entrypoint, with_project_test_env,
};

#[test]
fn channel_create_unbounded_default_lowers_to_clif() {
    with_project_test_env(&corelib_tests_project_root(), || {
        let artifact = lower_corelib_tests_entrypoint(
            "concurrency/ChannelApiTests.bd",
            "channel_create_unbounded_default",
        );
        assert!(
            !artifact.functions.is_empty(),
            "expected CLIF functions for channel test entrypoint"
        );
    });
}
