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
    call_abi_signature, call_lowering, child_nodes, enum_constructor, enum_layout, enum_match,
    generic_call_specialization, item_abi_signature, item_name, node_kind, project_session_for_syntax_assembly,
    reachable_items, typed_let_call_result, with_db,
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
corelib_lower_test!(
    progress_bar_clamps_percent_lowers,
    "console/ControlsProgressBarTests.bd",
    "progress_bar_clamps_percent"
);
corelib_lower_test!(system_error_writeline_smoke_lowers, "system/ErrorWriteTests.bd", "error_writeline_smoke");
corelib_lower_test!(system_input_read_smoke_lowers, "system/InputReadTests.bd", "input_read_returns_result");
corelib_lower_test!(system_input_read_line_lowers, "system/InputReadTests.bd", "input_read_line_returns_result");
corelib_lower_test!(system_input_is_empty_lowers, "system/InputReadTests.bd", "input_is_empty_is_boolean");
corelib_lower_test!(core_bytes_fill_lowers, "core/BytesTests.bd", "bytes_fill_sets_range");
corelib_lower_test!(core_bytes_from_string_lowers, "core/BytesTests.bd", "bytes_from_string_matches_utf8_length");
corelib_lower_test!(core_bytes_subslice_lowers, "core/BytesTests.bd", "bytes_subslice_copies_window");
corelib_lower_test!(core_bytes_compare_lowers, "core/BytesTests.bd", "bytes_compare_lexicographic");
corelib_lower_test!(
    core_utf8_decode_from_bytes_lowers,
    "core/EncodingUtf8Tests.bd",
    "utf8_decode_from_bytes_roundtrip"
);
corelib_lower_test!(
    collections_integration_stack_push_lowers,
    "collections/CollectionsTests.bd",
    "stack_push_updates_count"
);
corelib_lower_test!(collections_set_add_lowers, "collections/SetTests.bd", "set_add_increments_count");
corelib_lower_test!(core_hex_encode_bytes_lowers, "core/EncodingUtf8Tests.bd", "hex_encode_bytes_roundtrip");
corelib_lower_test!(core_base64_encode_bytes_lowers, "core/EncodingUtf8Tests.bd", "base64_encode_bytes_roundtrip");
corelib_lower_test!(
    core_base64_decode_invalid_length_lowers,
    "core/EncodingUtf8Tests.bd",
    "base64_decode_invalid_length_rejected"
);
corelib_lower_test!(core_math_floor_lowers, "core/MathTests.bd", "floor_positive");
corelib_lower_test!(core_math_sqrt_lowers, "core/MathTests.bd", "sqrt_perfect");
corelib_lower_test!(console_ansi_cursor_position_lowers, "console/AnsiBuildersTests.bd", "ansi_cursor_position_golden");
corelib_lower_test!(console_format_scan_trim_lowers, "console/FormatScanTests.bd", "trim_strips_spaces");
corelib_lower_test!(console_format_attributes_named_lowers, "console/FormatAttributesTests.bd", "parse_named_red");
corelib_lower_test!(
    console_format_attributes_list_lowers,
    "console/FormatAttributesTests.bd",
    "apply_attr_list_fg_and_bg"
);
corelib_lower_test!(
    console_controls_panel_lowers,
    "console/ControlsFrameTests.bd",
    "panel_unicode_frame_when_preferred"
);
corelib_lower_test!(system_time_now_utc_lowers, "system/TimeTests.bd", "time_now_utc_returns_instant");
corelib_lower_test!(system_time_monotonic_now_lowers, "system/TimeTests.bd", "time_monotonic_now_returns_instant");
corelib_lower_test!(text_casing_snake_to_pascal_lowers, "text/TextCasingTests.bd", "snake_to_pascal_maps_rule_names");
corelib_lower_test!(collections_stack_push_lowers, "collections/StackTests.bd", "stack_push_increments_count");
corelib_lower_test!(
    collections_set_contains_lowers,
    "collections/SetTests.bd",
    "set_contains_returns_false_for_missing"
);
corelib_lower_test!(core_args_all_lowers, "core/ArgsTests.bd", "args_all_returns_array");
corelib_lower_test!(core_random_seeded_lowers, "core/RandomTests.bd", "random_seeded_deterministic");

#[test]
fn text_casing_upper_char_has_direct_string_call_result_fact() {
    with_large_test_stack(|| {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("text/TextCasingTests.bd");
            with_db(|db| {
                let syntax_assembly =
                    Arc::new(SyntaxProgramAssembly::from(resolved.assembly.as_ref().expect("corelib entry assembly")));
                let project =
                    project_session_for_syntax_assembly(db, &syntax_assembly, "casing-call-fact", "prepared-casing")
                        .expect("corelib syntax project session");
                let generation = SyntaxGenerationId(141);
                let typed =
                    build_typed_program(db, project, generation, syntax_assembly).expect("corelib syntax program");
                let casing = typed
                    .assembly
                    .units()
                    .iter()
                    .find(|unit| unit.path.ends_with("Core/Text/Casing.bd"))
                    .expect("Casing source unit");
                let unit = SourceUnitId::new(db, casing.path.clone());
                let index = SyntaxIndex::from_program(&casing.program, generation);
                let call = index
                    .ids_of_kind(NodeKind::CallExpression)
                    .map(|node| AstNodeKey { unit, generation, node })
                    .find(|key| {
                        matches!(
                            call_lowering(db, *key),
                            Ok(Some(beskid_queries::CallLowering::Direct(declaration)))
                                if item_name(db, declaration).ok().flatten().as_deref() == Some("UpperChar")
                        )
                    })
                    .expect("direct String.UpperChar call");
                assert_eq!(
                    call_abi_signature(db, call).expect("UpperChar call ABI"),
                    Some(ItemSignature {
                        parameters: Arc::from([SemanticTypeId::I64]),
                        result: SemanticTypeId::STRING,
                    })
                );
                let typed_lets = (0..4096)
                    .map(|node| AstNodeKey { unit, generation, node: AstNodeId(node) })
                    .filter(|key| node_kind(db, *key).ok().flatten() == Some(IndexedNodeKind::LetStatement))
                    .filter_map(|key| typed_let_call_result(db, key).ok().flatten())
                    .collect::<Vec<_>>();
                assert!(
                    typed_lets.iter().any(|fact| fact.abi_type == SemanticTypeId::STRING),
                    "typed direct-call lets: {typed_lets:?}"
                );
                assert!(
                    typed_lets.iter().any(|fact| fact.initializer == call),
                    "UpperChar call={call:?}, typed direct-call lets={typed_lets:?}"
                );
            });
        });
    });
}

#[test]
fn text_parser_multi_payload_constructor_has_exact_fact() {
    with_large_test_stack(|| {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("text/TextParserCombinatorTests.bd");
            with_db(|db| {
                let syntax_assembly =
                    Arc::new(SyntaxProgramAssembly::from(resolved.assembly.as_ref().expect("corelib entry assembly")));
                let project =
                    project_session_for_syntax_assembly(db, &syntax_assembly, "parser-enum-fact", "prepared-parser")
                        .expect("corelib syntax project session");
                let generation = SyntaxGenerationId(142);
                let typed =
                    build_typed_program(db, project, generation, syntax_assembly).expect("corelib syntax program");
                let literals = typed
                    .assembly
                    .units()
                    .iter()
                    .find(|unit| unit.path.ends_with("Core/Text/Parser/Literals.bd"))
                    .expect("Parser Literals source unit");
                let unit = SourceUnitId::new(db, literals.path.clone());
                let constructors = (0..4096)
                    .map(|node| AstNodeKey { unit, generation, node: AstNodeId(node) })
                    .filter(|key| {
                        node_kind(db, *key).ok().flatten() == Some(IndexedNodeKind::EnumConstructorExpression)
                    })
                    .collect::<Vec<_>>();
                let constructor = constructors
                    .iter()
                    .copied()
                    .find(|key| enum_constructor(db, *key).ok().flatten().is_some_and(|fact| fact.payloads.len() == 2))
                    .unwrap_or_else(|| {
                        panic!(
                            "two-payload TextParseResult constructor facts: {:?}",
                            constructors
                                .iter()
                                .map(|key| {
                                    (
                                        key,
                                        enum_constructor(db, *key).map_err(|error| error.to_string()),
                                        enum_layout(db, *key).map_err(|error| error.to_string()),
                                    )
                                })
                                .collect::<Vec<_>>()
                        )
                    });
                assert_eq!(enum_constructor(db, constructor).expect("constructor fact").unwrap().payloads.len(), 2);
            });
        });
    });
}

#[test]
fn ansi_sgr_color_model_matches_have_exact_enum_facts() {
    with_large_test_stack(|| {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("console/AnsiSgrGoldenTests.bd");
            with_db(|db| {
                let syntax_assembly =
                    Arc::new(SyntaxProgramAssembly::from(resolved.assembly.as_ref().expect("corelib entry assembly")));
                let project =
                    project_session_for_syntax_assembly(db, &syntax_assembly, "sgr-enum-fact", "prepared-sgr-enum")
                        .expect("corelib syntax project session");
                let generation = SyntaxGenerationId(143);
                let typed =
                    build_typed_program(db, project, generation, syntax_assembly).expect("corelib syntax program");
                let sgr = typed
                    .assembly
                    .units()
                    .iter()
                    .find(|unit| unit.path.ends_with("Ansi/Sgr.bd"))
                    .expect("Ansi Sgr source unit");
                let unit = SourceUnitId::new(db, sgr.path.clone());
                let facts = (0..4096)
                    .map(|node| AstNodeKey { unit, generation, node: AstNodeId(node) })
                    .filter(|key| node_kind(db, *key).ok().flatten() == Some(IndexedNodeKind::MatchExpression))
                    .filter_map(|key| enum_match(db, key).ok().flatten())
                    .collect::<Vec<_>>();
                assert_eq!(facts.len(), 2, "Sgr ColorModel enum match facts: {facts:?}");
                assert!(facts.iter().all(|fact| fact.arms.len() == 3));
            });
        });
    });
}

#[test]
fn core_input_matches_have_enum_facts() {
    with_large_test_stack(|| {
        let root = corelib_tests_project_root();
        with_project_test_env(&root, || {
            let resolved = resolve_corelib_tests_entry_with_assembly("system/InputReadTests.bd");
            with_db(|db| {
                let syntax_assembly =
                    Arc::new(SyntaxProgramAssembly::from(resolved.assembly.as_ref().expect("corelib entry assembly")));
                let project = project_session_for_syntax_assembly(
                    db,
                    &syntax_assembly,
                    "core-input-match-facts",
                    "prepared-core-input-match-facts",
                )
                .expect("corelib syntax project session");
                let generation = SyntaxGenerationId(140);
                let typed =
                    build_typed_program(db, project, generation, syntax_assembly).expect("corelib syntax program");
                let input = typed
                    .assembly
                    .units()
                    .iter()
                    .find(|unit| unit.path.ends_with("Core/Input/Input.bd"))
                    .expect("Core.Input source unit");
                let unit = SourceUnitId::new(db, input.path.clone());
                let index = SyntaxIndex::from_program(&input.program, generation);
                for node in index.ids_of_kind(NodeKind::MatchExpression) {
                    let key = AstNodeKey { unit, generation, node };
                    assert!(
                        enum_match(db, key)
                            .unwrap_or_else(|error| panic!("enum match fact for {key:?}: {error}"))
                            .is_some(),
                        "missing enum match fact for {key:?}"
                    );
                }
            });
        });
    });
}
corelib_lower_test!(vertical_stack_render_lowers, "console/ControlsLayoutTests.bd", "vertical_stack_render_smoke");
corelib_lower_test!(vertical_stack_child_count_lowers, "console/ControlsLayoutTests.bd", "vertical_stack_child_count");
corelib_lower_test!(
    compiler_sdk_catalog_packages_array_lowers,
    "compiler-sdk/CompilerSdkSurfaceTests.bd",
    "catalog_packages_array_is_constructible"
);
corelib_lower_test!(
    compiler_sdk_workspace_summary_lowers,
    "compiler-sdk/CompilerSdkSurfaceTests.bd",
    "workspace_summary_fields_are_constructible"
);
corelib_lower_test!(hub_create_lowers, "concurrency/HubRegisterTests.bd", "hub_create_returns_handle");
corelib_lower_test!(hub_register_lowers, "concurrency/HubRegisterTests.bd", "hub_register_returns_result");
corelib_lower_test!(hub_unregister_lowers, "concurrency/HubRegisterTests.bd", "hub_unregister_does_not_panic");
corelib_lower_test!(concurrency_yield_lowers, "concurrency/ConcurrencyClockTests.bd", "yield_invokes_builtin");
corelib_lower_test!(slice_returns_substring_lowers, "console/FormatScanTests.bd", "slice_returns_substring");
corelib_lower_test!(text_cursor_from_starts_at_zero_lowers, "text/TextCursorTests.bd", "from_starts_at_zero");
corelib_lower_test!(text_parser_module_imports_lower, "text/TextParserTests.bd", "parser_module_imports");
corelib_lower_test!(text_parser_literal_match_lowers, "text/TextParserTests.bd", "literal_matches_using_cursor");
corelib_lower_test!(text_parser_satisfy_lowers, "text/TextParserCombinatorTests.bd", "satisfy_reads_one_code_unit");
corelib_lower_test!(
    text_regex_lower_class_quantifier_lowers,
    "text/TextRegexTests.bd",
    "lower_class_quantifier_matches_prefix"
);
corelib_lower_test!(text_regex_digit_class_lowers, "text/TextRegexTests.bd", "digit_class_finds_digits");
corelib_lower_test!(
    pest_grammar_lower_run_rule_lowers,
    "text/PestGrammarParseTests.bd",
    "parse_grammar_rules_reads_lower_run_rule"
);
corelib_lower_test!(
    pest_emit_parse_pat_callable_lowers,
    "text/PestEmitGoldenTests.bd",
    "emit_includes_parse_pat_callable"
);
corelib_lower_test!(collections_set_remove_lowers, "collections/SetTests.bd", "set_remove_decrements_count");
corelib_lower_test!(collections_stack_peek_lowers, "collections/StackTests.bd", "stack_peek_returns_last_pushed");
corelib_lower_test!(query_iterator_next_lowers, "query/QueryTests.bd", "array_iterator_next_returns_elements");
corelib_lower_test!(query_iterator_exhaustion_lowers, "query/QueryTests.bd", "array_iterator_exhaustion_returns_none");
corelib_lower_test!(concurrency_status_roundtrip_lowers, "concurrency/StatusAbiTests.bd", "status_roundtrip");
corelib_lower_test!(channel_send_result_lowers, "concurrency/ChannelApiTests.bd", "channel_send_returns_result");
corelib_lower_test!(mutex_lock_result_lowers, "concurrency/MutexTryLockTests.bd", "mutex_lock_returns_result");
corelib_lower_test!(mutex_try_lock_option_lowers, "concurrency/MutexTryLockTests.bd", "mutex_try_lock_returns_option");

#[test]
fn render_context_move_to_clif_has_no_unconditional_trap() {
    with_project_test_env(&corelib_tests_project_root(), || {
        let artifact =
            lower_corelib_tests_entrypoint("console/RenderContextTests.bd", "render_context_move_to_emits_csi");
        let clif = beskid_codegen::render_clif(&artifact);
        let entry = clif
            .split(";; Function: ")
            .find(|function| function.starts_with("render_context_move_to_emits_csi#"))
            .expect("render-context entry CLIF");
        let position = clif
            .split(";; Function: ")
            .find(|function| function.starts_with("Position#"))
            .expect("cursor-position CLIF");
        let index_of = clif
            .split(";; Function: ")
            .find(|function| function.starts_with("IndexOfFrom#"))
            .expect("string IndexOfFrom CLIF");
        assert!(
            !entry.lines().any(|line| line.trim_start().starts_with("trap ")),
            "entry has unconditional trap:\n{entry}"
        );
        assert!(
            !position.lines().any(|line| line.contains(" = iadd ")),
            "cursor interpolation added an integer to a string pointer instead of concatenating:\n{position}"
        );
        assert!(
            !index_of.contains("iconst.i8 -1"),
            "boolean NOT inverted every storage bit instead of toggling boolean truth:\n{index_of}"
        );
    });
}
