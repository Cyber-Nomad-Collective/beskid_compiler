//! CLIF lowering gates for selected `corelib_tests` entries (per-test link plan).
//!
//! Run serially (lowering reuses assembly cache but holds process-global locks):
//! `cargo test -p beskid_tests corelib_tests_codegen -- --nocapture --test-threads=1`

use crate::projects::fixture_harness::{
    corelib_tests_project_root, lower_corelib_tests_entrypoint, resolve_corelib_tests_entry_with_assembly,
    with_large_test_stack, with_project_test_env,
};

use std::collections::HashMap;
use std::sync::Arc;

use beskid_analysis::projects::SyntaxProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeId, AstNodeKey, IndexedNodeKind, ItemSignature, SemanticTypeId, SourceUnitId, build_typed_program,
    child_nodes, generic_call_specialization, item_abi_signature, item_name, node_kind,
    project_session_for_syntax_assembly, reachable_items, with_db,
};

#[test]
fn syscall_result_predicates_have_call_derived_pointer_specializations() {
    with_large_test_stack(|| {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("system/SyscallWriteTests.bd");
            with_db(|db| {
                let syntax_assembly =
                    Arc::new(SyntaxProgramAssembly::from(resolved.assembly.as_ref().expect("corelib entry assembly")));
                let project = project_session_for_syntax_assembly(
                    db,
                    &syntax_assembly,
                    "syscall-result-specialization",
                    "prepared-syscall-result-specialization",
                )
                .expect("corelib syntax project session");
                let generation = SyntaxGenerationId(139);
                let typed =
                    build_typed_program(db, project, generation, syntax_assembly).expect("corelib syntax program");
                let entry = typed.assembly.entry_unit();
                let unit = SourceUnitId::new(db, entry.path.clone());
                let index = SyntaxIndex::from_program(&entry.program, generation);
                let expected =
                    ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::BOOL };
                let matching = index
                    .ids_of_kind(NodeKind::CallExpression)
                    .filter_map(|node| {
                        generic_call_specialization(db, AstNodeKey { unit, generation, node }).ok().flatten()
                    })
                    .filter(|specialization| specialization.signature == expected)
                    .count();

                assert_eq!(matching, 4, "each Results.IsOk call must retain POINTER -> BOOL");

                let entry = index
                    .ids_of_kind(NodeKind::TestDefinition)
                    .map(|node| AstNodeKey { unit, generation, node })
                    .find(|key| {
                        item_name(db, *key).ok().flatten().as_deref()
                            == Some("syscall_write_empty_string_returns_non_negative")
                    })
                    .expect("syscall test entrypoint");
                let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
                let reachable =
                    reachable_items(db, root, entry).expect("reachable item query").expect("reachable item facts");
                let mut specializations = HashMap::<AstNodeKey, Vec<ItemSignature>>::new();
                let mut pending = reachable.to_vec();
                while let Some(key) = pending.pop() {
                    if let Some(specialization) =
                        generic_call_specialization(db, key).expect("generic call specialization")
                    {
                        specializations.entry(specialization.declaration).or_default().push(specialization.signature);
                    }
                    if let Some(children) = child_nodes(db, key).expect("child nodes") {
                        pending.extend(children.iter().copied());
                    }
                }
                let missing = reachable
                    .iter()
                    .copied()
                    .filter(|key| {
                        node_kind(db, *key).ok().flatten() == Some(IndexedNodeKind::FunctionDefinition)
                            && item_abi_signature(db, *key).ok().flatten().is_none()
                            && !specializations.contains_key(key)
                    })
                    .map(|key| {
                        format!(
                            "{}@{:?}",
                            item_name(db, key).ok().flatten().unwrap_or_else(|| Arc::from("<anon>")),
                            key
                        )
                    })
                    .collect::<Vec<_>>();
                assert!(missing.is_empty(), "generic items without specialization: {missing:?}");
            });
        });
    });
}

macro_rules! corelib_lower_test {
    ($name:ident, $entry:literal, $entrypoint:literal) => {
        #[test]
        fn $name() {
            with_project_test_env(&corelib_tests_project_root(), || {
                let artifact = lower_corelib_tests_entrypoint($entry, $entrypoint);
                assert!(!artifact.functions.is_empty(), "expected CLIF functions for {} in {}", $entrypoint, $entry);
            });
        }
    };
}

#[test]
fn channel_module_import_smoke_lowers_to_clif() {
    with_project_test_env(&corelib_tests_project_root(), || {
        let artifact =
            lower_corelib_tests_entrypoint("concurrency/ChannelApiTests.bd", "channel_create_returns_handle");
        assert!(!artifact.functions.is_empty(), "expected CLIF functions for channel test entrypoint");
    });
}

corelib_lower_test!(style_chain_bold_wraps_lowers, "console/AnsiStyleChainTests.bd", "style_chain_bold_wraps");
corelib_lower_test!(strip_bold_plain_lowers, "console/FormatMarkdownTests.bd", "format_module_imports");
corelib_lower_test!(parse_env_columns_lowers, "console/TerminalPlatformTests.bd", "parse_env_columns_known_values");
corelib_lower_test!(
    messages_channel_factory_lowers,
    "console/ConsoleMessageChannelTests.bd",
    "messages_channel_factory_smoke"
);
corelib_lower_test!(panel_ascii_frame_lowers, "console/ControlsPanelTests.bd", "panel_module_imports");
corelib_lower_test!(system_error_writeline_smoke_lowers, "system/ErrorWriteTests.bd", "error_writeline_smoke");
corelib_lower_test!(system_input_read_smoke_lowers, "system/InputReadTests.bd", "input_read_returns_result");
corelib_lower_test!(vertical_stack_render_lowers, "console/ControlsLayoutTests.bd", "vertical_stack_render_smoke");
corelib_lower_test!(hub_create_lowers, "concurrency/HubRegisterTests.bd", "hub_create_returns_handle");
corelib_lower_test!(slice_returns_substring_lowers, "console/FormatScanTests.bd", "slice_returns_substring");
corelib_lower_test!(text_cursor_from_starts_at_zero_lowers, "text/TextCursorTests.bd", "from_starts_at_zero");
corelib_lower_test!(text_parser_module_imports_lower, "text/TextParserTests.bd", "parser_module_imports");
