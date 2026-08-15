use std::fs;

use crate::projects::fixture_harness::{
    corelib_mvp_fixture, corelib_tests_project_root, resolve_corelib_tests_entry, resolve_fixture_with_assembly,
    shared_corelib_mvp_assembly, with_large_test_stack, with_project_test_env,
};
use crate::projects::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};
use beskid_analysis::projects::build_compile_plan;
use beskid_analysis::services::{
    analyze_file_in_project, analyze_source_in_project, parse_program, resolve_input, FrontEndOptions, PrepareOptions,
};
use beskid_analysis::Severity;
use beskid_queries::{program_assembly, with_db};

use super::{compiler_sdk_src, corelib_root, corelib_workspace_root, foundation_src, stratified_corelib_parse_samples};

/// Linux CI runners use a smaller default thread stack than macOS; corelib lowering needs more headroom.
// with_large_test_stack lives in fixture_harness

#[test]
fn checked_in_corelib_template_builds_compile_plan() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let manifest_path = corelib_root().join("corelib.bproj");
        let plan = build_compile_plan(&manifest_path, None).expect("corelib plan should build");

        assert_eq!(plan.project_name, "corelib");
        assert_eq!(plan.target.name, "__aggregate__");
        assert_eq!(plan.target.entry, None);
    });
}

#[test]
fn checked_in_corelib_sources_parse_as_beskid_programs() {
    let root = corelib_workspace_root();

    for relative in stratified_corelib_parse_samples() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read corelib source {}", path.display()));
        parse_program(&source)
            .unwrap_or_else(|err| panic!("corelib source should parse {}\nparse error: {err:#}", path.display()));
    }
}

#[test]
fn checked_in_corelib_syscall_file_does_not_report_module_resolution_false_positives() {
    with_project_test_env(&corelib_mvp_fixture(), || {
        let diagnostics =
            analyze_file_in_project(&corelib_mvp_fixture().join("Src/Main.bd")).expect("analyze corelib_mvp entry");

        assert!(
            diagnostics.iter().all(|diag| !matches!(diag.code.as_deref(), Some("E1105") | Some("E1108"))),
            "corelib_mvp entry should not emit module-path false positives: {diagnostics:#?}"
        );
    });
}

#[ignore = "corelib project diagnostics fixture currently fails parse/assembly on this branch"]
#[test]
fn checked_in_corelib_sources_do_not_emit_error_diagnostics_in_project_context() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let root = corelib_workspace_root();
        let relative = "packages/foundation/src/Core/Results/Results.bd";
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read corelib source {}", path.display()));
        let diagnostics =
            analyze_source_in_project(&path, &source).unwrap_or_else(|_| panic!("analyze {}", path.display()));
        let errors: Vec<_> = diagnostics.into_iter().filter(|diag| matches!(diag.severity, Severity::Error)).collect();
        assert!(errors.is_empty(), "expected no error diagnostics for {} but got: {errors:#?}", path.display());
    });
}

#[test]
fn corelib_mvp_fixture_resolves_std_modules_via_program_assembly() {
    with_project_test_env(&corelib_mvp_fixture(), || {
        let assembly = shared_corelib_mvp_assembly();
        let resolution = assembly
            .module_index
            .resolve_entry(&assembly.entry_unit().program)
            .expect("cross-module resolve via ModuleIndex");

        assert!(
            resolution.items.iter().any(|item| item.name == "WriteLine"),
            "expected WriteLine from Std.Core.Output in merged resolution"
        );
        assert!(
            resolution.items.iter().any(|item| item.name == "Len"),
            "expected Len from Std.Core.String in merged resolution"
        );
    });
}

#[test]
fn corelib_mvp_fixture_lowers_via_program_assembly() {
    with_large_test_stack(|| {
        with_project_test_env(&corelib_mvp_fixture(), || {
            let resolved = resolve_fixture_with_assembly(&corelib_mvp_fixture(), "src/Main.bd", "App");
            beskid_queries::prepare_compilation(
                &resolved,
                PrepareOptions {
                    front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                    ..Default::default()
                },
                None,
            )
            .expect("corelib_mvp should resolve and type-check with semantic facts");
        });
    });
}

#[test]
fn corelib_assembly_typechecks_nested_qualified_result_arguments_and_predicates() {
    with_large_test_stack(|| {
        let project = corelib_tests_project_root();
        with_project_test_env(&project, || {
            let source = r#"
use Core.Syscall;
use Core.Results;

i32 Main() {
    Core.Results.Result<i64, Core.Syscall.SyscallError> result =
        Core.Syscall.Write(-1_i64, "x");
    if Results.IsOk(result) {
        return 1;
    }
    if Results.IsError(result) {
        return 0;
    }
    return 2;
}
"#;
            let mut resolved = resolve_corelib_tests_entry("system/SyscallErgonomicsTests.bd");
            resolved.source = source.into();
            let plan = resolved.compile_plan.clone().expect("corelib tests compile plan");
            let options = beskid_analysis::projects::assembly_options_for_plan(&plan);
            let assembly = with_db(|db| {
                program_assembly(
                    db,
                    &plan,
                    resolved.prepared_workspace.as_ref(),
                    &resolved.source_path,
                    Some(source),
                    &options,
                )
            })
            .expect("nested generic regression assembly");

            resolved.assembly = Some(assembly);
            beskid_queries::prepare_compilation(
                &resolved,
                PrepareOptions {
                    front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                    ..Default::default()
                },
                None,
            )
            .expect(
                "assembly should typecheck Core.Results.Result<i64, Core.Syscall.SyscallError> \
                 through Results.IsOk and Results.IsError semantic facts",
            );
        });
    });
}

#[test]
fn corelib_mvp_fixture_entry_does_not_emit_module_resolution_false_positives() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture_main = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
        let diagnostics = analyze_file_in_project(&fixture_main).expect("analyze corelib_mvp fixture");

        assert!(
            diagnostics.iter().all(|diag| !matches!(diag.code.as_deref(), Some("E1105") | Some("E1108"))),
            "corelib_mvp fixture should not emit module-path false positives: {diagnostics:#?}"
        );
    });
}

#[test]
fn checked_in_corelib_aggregate_entry_is_workspace_placeholder() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let project = corelib_root();
        let resolved =
            resolve_input(None, Some(&project), None, None, false, false).expect("resolve corelib aggregate project");
        let plan = resolved.compile_plan.expect("compile plan");
        assert_eq!(plan.target.name, "__aggregate__");
        assert!(plan.target.entry.is_none());
    });
}

#[test]
fn checked_in_compiler_sdk_syntax_parses_as_beskid_program() {
    let entry = compiler_sdk_src().join("Beskid/Syntax.bd");
    let source = fs::read_to_string(&entry).expect("read compiler-sdk syntax facade");
    parse_program(&source).expect("compiler-sdk syntax facade should parse");
}

#[test]
fn checked_in_compiler_sdk_syntax_exports_node_inventory() {
    let syntax =
        fs::read_to_string(compiler_sdk_src().join("Beskid/Syntax.bd")).expect("read compiler-sdk syntax facade");
    assert!(
        syntax.contains("pub mod Beskid.Syntax.Nodes;"),
        "compiler-sdk syntax facade should export Beskid.Syntax.Nodes"
    );
    assert!(
        !syntax.contains("pub mod Beskid.Compiler.Emit;"),
        "compiler-sdk syntax facade should not export legacy Beskid.Compiler.Emit"
    );
}

#[test]
fn checked_in_compiler_sdk_collect_parses_with_named_enum_payloads() {
    let collect = compiler_sdk_src().join("Beskid/Compiler/Collect.bd");
    let source = fs::read_to_string(&collect).expect("read Beskid.Compiler.Collect");
    assert!(
        source.contains("ContractDefinition(ContractDefinition definition)"),
        "Collect should declare named enum payloads for SyntaxContributionItem"
    );
    parse_program(&source).expect("Beskid.Compiler.Collect should parse");
}

#[test]
fn checked_in_compiler_sdk_emitter_hub_exports_split_modules() {
    let hub = fs::read_to_string(compiler_sdk_src().join("Beskid/Compiler/Emitter.bd")).expect("read Emitter hub");
    for needle in [
        "pub mod Beskid.Compiler.Emitter.Nodes",
        "pub mod Beskid.Compiler.Emitter.Contracts",
        "pub mod Beskid.Compiler.Emitter.Items",
        "pub mod Beskid.Compiler.Emitter.Contribution",
        "pub string EmitterFacadeVersion()",
    ] {
        assert!(hub.contains(needle), "Emitter hub missing `{needle}`");
    }
    parse_program(&hub).expect("Emitter hub should parse");
}

#[test]
fn checked_in_compiler_sdk_emitter_contribution_helpers_exist() {
    let contribution = fs::read_to_string(compiler_sdk_src().join("Beskid/Compiler/Emitter/Contribution.bd"))
        .expect("read Emitter.Contribution");
    for needle in [
        "pub GeneratedSyntaxContribution Empty()",
        "pub GeneratedSyntaxContribution AppendCode(",
        "pub CodeContribution CodeOutput(",
    ] {
        assert!(contribution.contains(needle), "Emitter.Contribution missing `{needle}`");
    }
    parse_program(&contribution).expect("Emitter.Contribution should parse");
}

#[test]
fn checked_in_compiler_sdk_collect_declares_mod_contracts() {
    let collect = fs::read_to_string(compiler_sdk_src().join("Beskid/Compiler/Collect.bd"))
        .expect("read Beskid.Compiler.Collect");

    for contract_name in [
        "pub contract Collector",
        "pub contract Generator",
        "pub contract Analyzer",
        "pub contract Rewriter",
        "pub contract AttributeGenerator",
    ] {
        assert!(collect.contains(contract_name), "Collect facade missing {contract_name}");
    }
    for method_shape in ["Collect(", "Generate(", "Analyze(", "Rewrite(TSourceNode sourceNode)", "Attributes("] {
        assert!(collect.contains(method_shape), "Collect facade missing method shape {method_shape}");
    }
}

#[test]
fn checked_in_corelib_mvp_modules_reference_runtime_backed_symbols() {
    let results_mod = fs::read_to_string(foundation_src().join("Core/Results/Results.bd")).expect("read Core.Results");
    let string_mod = fs::read_to_string(foundation_src().join("Core/String/String.bd")).expect("read Core.String");
    let string_core_mod =
        fs::read_to_string(foundation_src().join("Core/String/Core.bd")).expect("read Core.String.Core");
    let output_mod = fs::read_to_string(foundation_src().join("Core/Output/Output.bd")).expect("read Core.Output");

    assert!(results_mod.contains("pub enum Result"), "Core.Results should define Result enum");
    assert!(
        results_mod.contains("Ok(") && results_mod.contains("Error("),
        "Core.Results should expose Ok/Error variants"
    );
    assert!(
        results_mod.contains("pub bool IsOk") && results_mod.contains("pub bool IsError"),
        "Core.Results should expose Ok/Error predicates"
    );
    assert!(string_core_mod.contains("__str_len"), "Core.String.Core should use __str_len runtime builtin");
    assert!(
        string_mod.contains("pub mod Core.String.Core;") && string_mod.contains("Core.Len(text)"),
        "Core.String hub should re-export Core.String.Core and delegate Len to it"
    );
    let array_mod = fs::read_to_string(foundation_src().join("Collections/Array.bd")).expect("read Array");
    assert!(array_mod.contains("__array_len"), "Collections.Array should use __array_len for slice length");
    assert!(!output_mod.contains("__sys_print"), "Core.Output must not reference purged __sys_print builtins");
    assert!(
        output_mod.contains("Core.Syscall.WriteWith") && output_mod.contains("WriteLine"),
        "Core.Output should route through Core.Syscall.WriteWith and expose WriteLine"
    );
    let syscall_mod = fs::read_to_string(foundation_src().join("Core/Syscall/Syscall.bd")).expect("read Core.Syscall");
    assert!(syscall_mod.contains("__syscall_write"), "Core.Syscall should call __syscall_write builtin");
    assert!(syscall_mod.contains("__syscall_read"), "Core.Syscall should call __syscall_read builtin");
}

#[test]
fn checked_in_corelib_compiler_sdk_exports_version_tokens() {
    let compilation = fs::read_to_string(compiler_sdk_src().join("Beskid/Compiler/Compilation.bd"))
        .expect("read Beskid.Compiler.Compilation");
    assert!(
        compilation.contains("CompilerLanguageVersionToken"),
        "Compilation facade should expose language version token"
    );
    assert!(
        compilation.contains("SemanticSnapshotFamilyToken"),
        "Compilation facade should expose semantic snapshot family token"
    );
}

#[test]
fn checked_in_compiler_sdk_query_facade_contract_first_nodes() {
    let query =
        fs::read_to_string(compiler_sdk_src().join("Beskid/Compiler/Query.bd")).expect("read Beskid.Compiler.Query");
    let syntax = fs::read_to_string(compiler_sdk_src().join("Beskid/Syntax.bd")).expect("read Beskid.Syntax");
    let node_contract = fs::read_to_string(compiler_sdk_src().join("Beskid/Syntax/Nodes/Node.bd"))
        .expect("read Beskid.Syntax.Nodes.Node");
    let node_span = fs::read_to_string(compiler_sdk_src().join("Beskid/Syntax/Nodes/NodeSpan.bd"))
        .expect("read Beskid.Syntax.Nodes.NodeSpan");
    let node_list = fs::read_to_string(compiler_sdk_src().join("Beskid/Syntax/Nodes/NodeList.bd"))
        .expect("read Beskid.Syntax.Nodes.NodeList");

    assert!(
        query.contains(r#"return "0.4.0";"#),
        "Query facade version should be 0.4.0 after span + pipeline expansion"
    );
    assert!(syntax.contains(r#"return "0.4.0";"#), "Syntax facade version should be 0.4.0");
    assert!(!query.contains("pub type ReflectStub"), "Query facade must not declare ReflectStub placeholders");
    assert!(!query.contains("ReflectSdk"), "Query facade must not use legacy ReflectSdk* tokens");
    assert!(
        node_contract.contains("pub contract Node"),
        "syntax navigation surface must be the Node contract, not an item enum"
    );
    assert!(
        !node_contract.contains("pub enum Node"),
        "mirrored item wrapper enum must not be emitted into the Mod SDK"
    );
    assert!(
        node_list.contains("Beskid.Syntax.Nodes.NodeRef head"),
        "NodeList must carry NodeRef handles for program items"
    );
    assert!(
        node_contract.contains("Beskid.Syntax.Nodes.NodeSpan Span();"),
        "Node contract should expose span metadata"
    );
    assert!(node_span.contains("pub type NodeSpan"), "NodeSpan contract type should be generated");
    for api in [
        "pub SyntaxQuery At(",
        "pub SyntaxQuery AtProgram(",
        "pub Beskid.Syntax.Nodes.NodeRef[] Descendants(",
        "pub Option<Beskid.Syntax.Nodes.NodeRef> Parent(",
        "pub Beskid.Syntax.Nodes.NodeSpan Span(",
        "pub Option<Beskid.Syntax.Nodes.NodeSpan> TrySpan(",
        "pub SyntaxPipeline Pipeline(",
        "pub SyntaxPipeline Replace(",
        "pub Beskid.Syntax.Nodes.NodeRef Apply(",
        "pub Option<FunctionDefinition> AsFunctionDefinition(",
    ] {
        assert!(query.contains(api), "Query facade missing API: {api}");
    }
}

#[test]
fn checked_in_corelib_beskid_test_sources_parse() {
    let root = corelib_root();
    let test_files = [
        root.join("tests/corelib_tests/src/system/SyscallWriteTests.bd"),
        root.join("tests/corelib_tests/src/system/SyscallApiTests.bd"),
        root.join("tests/corelib_tests/src/system/SyscallErgonomicsTests.bd"),
        root.join("tests/corelib_tests/src/system/OutputWriteLineTests.bd"),
        root.join("tests/corelib_tests/src/system/InputReadTests.bd"),
        root.join("tests/corelib_tests/src/system/ErrorWriteTests.bd"),
        root.join("tests/corelib_tests/src/system/FsTests.bd"),
        root.join("tests/corelib_tests/src/system/PathTests.bd"),
        root.join("tests/corelib_tests/src/system/TimeTests.bd"),
        root.join("tests/corelib_tests/src/core/ResultsTests.bd"),
        root.join("tests/corelib_tests/src/core/OptionalTests.bd"),
        root.join("tests/corelib_tests/src/collections/ArrayTests.bd"),
        root.join("tests/corelib_tests/src/collections/CollectionsTier1Tests.bd"),
        root.join("tests/corelib_tests/src/collections/CollectionsTests.bd"),
        root.join("tests/corelib_tests/src/query/QueryTests.bd"),
        root.join("tests/corelib_tests/src/collections/ListTests.bd"),
        root.join("tests/corelib_tests/src/collections/MapTests.bd"),
        root.join("tests/corelib_tests/src/collections/SetTests.bd"),
        root.join("tests/corelib_tests/src/collections/QueueTests.bd"),
        root.join("tests/corelib_tests/src/collections/StackTests.bd"),
        root.join("tests/corelib_tests/src/console/AnsiEscapeTests.bd"),
        root.join("tests/corelib_tests/src/console/AnsiStyleChainTests.bd"),
        root.join("tests/corelib_tests/src/console/AnsiSgrGoldenTests.bd"),
        root.join("tests/corelib_tests/src/console/AnsiBuildersTests.bd"),
        root.join("tests/corelib_tests/src/console/FormatMarkdownTests.bd"),
        root.join("tests/corelib_tests/src/console/FormatAttributesTests.bd"),
        root.join("tests/corelib_tests/src/console/FormatScanTests.bd"),
        root.join("tests/corelib_tests/src/console/CapabilitiesTests.bd"),
        root.join("tests/corelib_tests/src/console/TerminalPlatformTests.bd"),
        root.join("tests/corelib_tests/src/console/ConsoleFacadeTests.bd"),
        root.join("tests/corelib_tests/src/console/ConsoleMessageChannelTests.bd"),
        root.join("tests/corelib_tests/src/console/ConsoleStyleTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsPanelTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsProgressBarTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsLayoutTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsFrameTests.bd"),
        root.join("tests/corelib_tests/src/console/RenderContextTests.bd"),
    ];
    for path in test_files {
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("read corelib test source {}", path.display()));
        parse_program(&source)
            .unwrap_or_else(|err| panic!("corelib test source should parse {}\nparse error: {err:#}", path.display()));
    }
}
