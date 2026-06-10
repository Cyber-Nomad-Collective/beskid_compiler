//! Front-end type-check gates for all `corelib_tests` entries.
//!
//! # Running
//!
//! **CI / full gate** (one shared Salsa session, ~minutes not hours):
//! ```bash
//! cargo test -p beskid_tests corelib_tests_front_end_typechecks_matrix -- --nocapture --test-threads=1
//! ```
//!
//! **Fast local smoke** (5 representative entries):
//! ```bash
//! BESKID_CORELIB_SPINE_SMOKE=1 cargo test -p beskid_tests corelib_tests_front_end_typechecks_matrix -- --nocapture --test-threads=1
//! ```
//!
//! **Skip entirely** (local iteration on unrelated crates):
//! ```bash
//! BESKID_SKIP_CORELIB_SPINE=1 cargo test -p beskid_tests
//! ```
//!
//! **Bisect one entry** (ignored per-entry tests):
//! ```bash
//! cargo test -p beskid_tests text_cursor_tests_front_end_typechecks -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Expected durations (debug build, warm Salsa disk cache, `--test-threads=1`):
//! - Single entry semantic gate: ~5–60s (assembly ~5s + gate typecheck)
//! - Full matrix (~44 entries): ~5–15 min
//! - Legacy executable prepare per entry was ~25 min — do not use for spine gates.

use crate::projects::fixture_harness::{
    corelib_tests_project_root, typecheck_corelib_tests_entry, with_project_test_env,
};

use super::corelib_spine_harness::run_corelib_typecheck_matrix;

#[test]
fn corelib_tests_front_end_typechecks_matrix() {
    run_corelib_typecheck_matrix();
}

macro_rules! corelib_typecheck_test {
    ($name:ident, $entry:literal) => {
        #[test]
        #[ignore = "bisect helper; CI uses corelib_tests_front_end_typechecks_matrix"]
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
    system_syscall_write_tests_front_end_typechecks,
    "system/SyscallWriteTests.bd"
);
corelib_typecheck_test!(
    system_syscall_api_tests_front_end_typechecks,
    "system/SyscallApiTests.bd"
);
corelib_typecheck_test!(
    system_syscall_ergonomics_tests_front_end_typechecks,
    "system/SyscallErgonomicsTests.bd"
);
corelib_typecheck_test!(
    system_output_write_line_tests_front_end_typechecks,
    "system/OutputWriteLineTests.bd"
);
corelib_typecheck_test!(
    system_output_write_tests_front_end_typechecks,
    "system/OutputWriteTests.bd"
);
corelib_typecheck_test!(
    console_ansi_escape_tests_front_end_typechecks,
    "console/AnsiEscapeTests.bd"
);
corelib_typecheck_test!(
    console_ansi_style_chain_tests_front_end_typechecks,
    "console/AnsiStyleChainTests.bd"
);
corelib_typecheck_test!(
    console_format_markdown_tests_front_end_typechecks,
    "console/FormatMarkdownTests.bd"
);
corelib_typecheck_test!(
    console_ansi_sgr_golden_tests_front_end_typechecks,
    "console/AnsiSgrGoldenTests.bd"
);
corelib_typecheck_test!(
    console_controls_panel_tests_front_end_typechecks,
    "console/ControlsPanelTests.bd"
);
corelib_typecheck_test!(
    controls_progress_bar_tests_front_end_typechecks,
    "console/ControlsProgressBarTests.bd"
);
corelib_typecheck_test!(
    console_controls_layout_tests_front_end_typechecks,
    "console/ControlsLayoutTests.bd"
);
corelib_typecheck_test!(
    system_input_read_tests_front_end_typechecks,
    "system/InputReadTests.bd"
);
corelib_typecheck_test!(
    system_error_write_tests_front_end_typechecks,
    "system/ErrorWriteTests.bd"
);
corelib_typecheck_test!(
    core_results_tests_front_end_typechecks,
    "core/ResultsTests.bd"
);
corelib_typecheck_test!(
    core_bytes_tests_front_end_typechecks,
    "core/BytesTests.bd"
);
corelib_typecheck_test!(
    core_encoding_utf8_tests_front_end_typechecks,
    "core/EncodingUtf8Tests.bd"
);
corelib_typecheck_test!(
    core_expression_body_tests_front_end_typechecks,
    "core/ExpressionBodyTests.bd"
);
corelib_typecheck_test!(
    compiler_sdk_surface_tests_front_end_typechecks,
    "compiler-sdk/CompilerSdkSurfaceTests.bd"
);
corelib_typecheck_test!(
    compiler_sdk_emitter_tests_front_end_typechecks,
    "compiler-sdk/CompilerSdkEmitterTests.bd"
);
corelib_typecheck_test!(
    concurrency_status_abi_tests_front_end_typechecks,
    "concurrency/StatusAbiTests.bd"
);
corelib_typecheck_test!(
    collections_array_tests_front_end_typechecks,
    "collections/ArrayTests.bd"
);
corelib_typecheck_test!(
    collections_tier1_tests_front_end_typechecks,
    "collections/CollectionsTier1Tests.bd"
);
corelib_typecheck_test!(
    collections_list_tests_front_end_typechecks,
    "collections/ListTests.bd"
);
corelib_typecheck_test!(
    collections_map_tests_front_end_typechecks,
    "collections/MapTests.bd"
);
corelib_typecheck_test!(
    collections_set_tests_front_end_typechecks,
    "collections/SetTests.bd"
);
corelib_typecheck_test!(
    collections_queue_tests_front_end_typechecks,
    "collections/QueueTests.bd"
);
corelib_typecheck_test!(
    collections_stack_tests_front_end_typechecks,
    "collections/StackTests.bd"
);
corelib_typecheck_test!(
    system_fs_tests_front_end_typechecks,
    "system/FsTests.bd"
);
corelib_typecheck_test!(
    system_path_tests_front_end_typechecks,
    "system/PathTests.bd"
);
corelib_typecheck_test!(
    system_time_tests_front_end_typechecks,
    "system/TimeTests.bd"
);
corelib_typecheck_test!(
    channel_api_tests_front_end_typechecks,
    "concurrency/ChannelApiTests.bd"
);
corelib_typecheck_test!(
    mutex_try_lock_tests_front_end_typechecks,
    "concurrency/MutexTryLockTests.bd"
);
corelib_typecheck_test!(
    concurrency_clock_tests_front_end_typechecks,
    "concurrency/ConcurrencyClockTests.bd"
);
corelib_typecheck_test!(
    hub_register_tests_front_end_typechecks,
    "concurrency/HubRegisterTests.bd"
);
corelib_typecheck_test!(
    wait_group_tests_front_end_typechecks,
    "concurrency/WaitGroupTests.bd"
);
corelib_typecheck_test!(
    fiber_handle_tests_front_end_typechecks,
    "concurrency/FiberHandleTests.bd"
);
corelib_typecheck_test!(
    console_message_channel_tests_front_end_typechecks,
    "console/ConsoleMessageChannelTests.bd"
);
corelib_typecheck_test!(
    console_capabilities_tests_front_end_typechecks,
    "console/CapabilitiesTests.bd"
);
corelib_typecheck_test!(
    console_terminal_platform_tests_front_end_typechecks,
    "console/TerminalPlatformTests.bd"
);
corelib_typecheck_test!(
    console_facade_tests_front_end_typechecks,
    "console/ConsoleFacadeTests.bd"
);
corelib_typecheck_test!(
    console_format_attributes_tests_front_end_typechecks,
    "console/FormatAttributesTests.bd"
);
corelib_typecheck_test!(
    console_format_scan_tests_front_end_typechecks,
    "console/FormatScanTests.bd"
);
corelib_typecheck_test!(
    console_style_tests_front_end_typechecks,
    "console/ConsoleStyleTests.bd"
);
corelib_typecheck_test!(
    console_controls_frame_tests_front_end_typechecks,
    "console/ControlsFrameTests.bd"
);
corelib_typecheck_test!(
    console_ansi_builders_tests_front_end_typechecks,
    "console/AnsiBuildersTests.bd"
);
corelib_typecheck_test!(
    console_render_context_tests_front_end_typechecks,
    "console/RenderContextTests.bd"
);
corelib_typecheck_test!(
    text_cursor_tests_front_end_typechecks,
    "text/TextCursorTests.bd"
);
corelib_typecheck_test!(
    text_parser_tests_front_end_typechecks,
    "text/TextParserTests.bd"
);
corelib_typecheck_test!(
    text_regex_tests_front_end_typechecks,
    "text/TextRegexTests.bd"
);
corelib_typecheck_test!(
    core_optional_tests_front_end_typechecks,
    "core/OptionalTests.bd"
);
corelib_typecheck_test!(
    collections_tests_front_end_typechecks,
    "collections/CollectionsTests.bd"
);
corelib_typecheck_test!(
    query_tests_front_end_typechecks,
    "query/QueryTests.bd"
);
