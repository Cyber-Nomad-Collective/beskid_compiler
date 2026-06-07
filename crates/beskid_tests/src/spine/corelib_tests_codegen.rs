//! CLIF lowering gates for selected `corelib_tests` entries (per-test link plan).

use crate::projects::fixture_harness::{
    corelib_tests_project_root, lower_corelib_tests_entrypoint, with_project_test_env,
};

macro_rules! corelib_lower_test {
    ($name:ident, $entry:literal, $entrypoint:literal) => {
        #[test]
        fn $name() {
            with_project_test_env(&corelib_tests_project_root(), || {
                let artifact = lower_corelib_tests_entrypoint($entry, $entrypoint);
                assert!(
                    !artifact.functions.is_empty(),
                    "expected CLIF functions for {} in {}",
                    $entrypoint,
                    $entry
                );
            });
        }
    };
}

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

corelib_lower_test!(
    style_chain_bold_wraps_lowers,
    "console/AnsiStyleChainTests.bd",
    "style_chain_bold_wraps"
);
corelib_lower_test!(
    strip_bold_plain_lowers,
    "console/FormatMarkdownTests.bd",
    "strip_bold_plain"
);
corelib_lower_test!(
    parse_env_columns_lowers,
    "console/TerminalPlatformTests.bd",
    "parse_env_columns_known_values"
);
corelib_lower_test!(
    messages_channel_factory_lowers,
    "console/ConsoleMessageChannelTests.bd",
    "messages_channel_factory"
);
corelib_lower_test!(
    panel_ascii_frame_lowers,
    "console/ControlsPanelTests.bd",
    "panel_ascii_frame_uses_plus_corners"
);
corelib_lower_test!(
    system_error_writeline_smoke_lowers,
    "system/ErrorWriteTests.bd",
    "error_writeline_smoke"
);
corelib_lower_test!(
    system_input_read_smoke_lowers,
    "system/InputReadTests.bd",
    "input_read_smoke"
);
corelib_lower_test!(
    vertical_stack_render_lowers,
    "console/ControlsLayoutTests.bd",
    "vertical_stack_render_joins_lines"
);
corelib_lower_test!(
    hub_register_accepts_channel_lowers,
    "concurrency/HubRegisterTests.bd",
    "hub_register_accepts_channel"
);
corelib_lower_test!(
    slice_returns_substring_lowers,
    "console/FormatScanTests.bd",
    "slice_returns_substring"
);

#[test]
fn dump_repeat_clif() {
    use crate::projects::fixture_harness::{corelib_tests_project_root, lower_corelib_tests_entrypoint, with_project_test_env};
    with_project_test_env(&corelib_tests_project_root(), || {
        let artifact = lower_corelib_tests_entrypoint("console/ControlsFrameTests.bd", "repeat_builds_string");
        for f in &artifact.functions {
            if f.name.contains("Repeat") {
                eprintln!("=== {} ===
{}", f.name, f.function);
            }
        }
    });
}
