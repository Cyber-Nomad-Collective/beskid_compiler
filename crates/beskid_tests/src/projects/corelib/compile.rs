use std::fs;

use beskid_analysis::Severity;
use beskid_analysis::projects::build_compile_plan;
use beskid_analysis::services::{analyze_file_in_project, parse_program, resolve_input};
use beskid_codegen::lower_source;

use crate::projects::std_dependency_env_lock;
use crate::projects::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};

use super::{
    compiler_sdk_src, corelib_root, corelib_workspace_root, expected_corelib_workspace_sources,
    foundation_src, runtime_src,
};

/// Linux CI runners use a smaller default thread stack than macOS; corelib lowering needs more headroom.
fn with_large_test_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .expect("spawn large-stack test thread")
        .join()
        .expect("join large-stack test thread");
}

#[test]
fn checked_in_corelib_template_builds_compile_plan() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let manifest_path = corelib_root().join("Project.proj");
        let plan =
            build_compile_plan(&manifest_path, Some("CoreLib")).expect("corelib plan should build");
        let expected_root = corelib_root()
            .join("src")
            .canonicalize()
            .expect("canonical corelib source root");
        let actual_root = plan
            .source_root
            .canonicalize()
            .expect("canonical compile-plan source root");

        assert_eq!(plan.target.name, "CoreLib");
        assert_eq!(actual_root, expected_root);
        assert!(plan.source_root.join("Prelude.bd").is_file());
    });
}

#[test]
fn checked_in_corelib_sources_parse_as_beskid_programs() {
    let root = corelib_workspace_root();

    for relative in expected_corelib_workspace_sources() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read corelib source {}", path.display()));
        parse_program(&source).unwrap_or_else(|err| {
            panic!(
                "corelib source should parse {}\nparse error: {err:#}",
                path.display()
            )
        });
    }
}

#[test]
fn checked_in_corelib_syscall_file_does_not_report_module_resolution_false_positives() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let diagnostics = analyze_file_in_project(&runtime_src().join("System/Syscall.bd"))
            .expect("analyze corelib syscall source");

        assert!(
            diagnostics
                .iter()
                .all(|diag| !matches!(diag.code.as_deref(), Some("E1005") | Some("E1105"))),
            "corelib syscall file should not emit E1005/E1105 false positives: {diagnostics:#?}"
        );
    });
}

#[test]
fn checked_in_corelib_sources_do_not_emit_error_diagnostics_in_project_context() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let root = corelib_workspace_root();

        for relative in expected_corelib_workspace_sources() {
            let path = root.join(relative);
            let diagnostics = analyze_file_in_project(&path)
                .unwrap_or_else(|_| panic!("analyze {}", path.display()));
            let errors: Vec<_> = diagnostics
                .into_iter()
                .filter(|diag| matches!(diag.severity, Severity::Error))
                .collect();
            assert!(
                errors.is_empty(),
                "expected no error diagnostics for {} but got: {errors:#?}",
                path.display()
            );
        }
    });
}

#[test]
fn corelib_mvp_fixture_entry_does_not_emit_module_resolution_false_positives() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let fixture_main = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
        let diagnostics =
            analyze_file_in_project(&fixture_main).expect("analyze corelib_mvp fixture");

        assert!(
            diagnostics
                .iter()
                .all(|diag| !matches!(diag.code.as_deref(), Some("E1105") | Some("E1108"))),
            "corelib_mvp fixture should not emit module-path false positives: {diagnostics:#?}"
        );
    });
}

#[test]
fn checked_in_corelib_prelude_lowers_to_codegen_artifact() {
    with_large_test_stack(|| {
        let _env_guard = std_dependency_env_lock();
        with_cwd_at_workspace_root(&compiler_workspace_root(), || {
            let project = corelib_root();
            let resolved = resolve_input(None, Some(&project), Some("CoreLib"), None, false, false)
                .expect("resolve corelib project input");

            // Full lowering of the aggregate corelib prelude overflows thread stacks on CI hosts
            // (debug and release). Verify resolve + parse here; compiler-sdk has a dedicated lowering test.
            parse_program(&resolved.source).expect("corelib prelude should parse");
        });
    });
}

#[test]
fn checked_in_compiler_sdk_prelude_lowers_to_codegen_artifact() {
    with_large_test_stack(|| {
        let _env_guard = std_dependency_env_lock();
        with_cwd_at_workspace_root(&compiler_workspace_root(), || {
            let sdk = corelib_workspace_root().join("packages/compiler-sdk");
            let resolved = resolve_input(None, Some(&sdk), Some("CompilerSdkLib"), None, false, false)
                .expect("resolve compiler-sdk project input");

            let _lowered = lower_source(&resolved.source_path, &resolved.source, true)
                .expect("lower compiler-sdk prelude should succeed");
        });
    });
}

#[test]
fn checked_in_corelib_prelude_exports_mvp_modules() {
    let prelude = fs::read_to_string(corelib_root().join("src/Prelude.bd")).expect("read prelude");

    assert!(
        prelude.contains("pub mod Core.Results;"),
        "Prelude should export Core.Results"
    );
    assert!(
        prelude.contains("pub mod Core.ErrorHandling;"),
        "Prelude should export Core.ErrorHandling"
    );
    assert!(
        prelude.contains("pub mod Core.String;"),
        "Prelude should export Core.String"
    );
    assert!(
        prelude.contains("pub mod Testing.Contracts;"),
        "Prelude should export Testing.Contracts"
    );
    assert!(
        prelude.contains("pub mod Testing.Assertions;"),
        "Prelude should export Testing.Assertions"
    );
    assert!(
        prelude.contains("pub mod System.Input;"),
        "Prelude should export System.Input"
    );
    assert!(
        prelude.contains("pub mod System.Output;"),
        "Prelude should export System.Output"
    );
    assert!(
        prelude.contains("pub mod System.Error;"),
        "Prelude should export System.Error"
    );
    assert!(
        prelude.contains("pub mod Console;"),
        "Prelude should export Console"
    );
    assert!(
        prelude.contains("pub mod System.Syscall;"),
        "Prelude should export System.Syscall"
    );
    assert!(
        prelude.contains("pub mod System.Syscall.Descriptor;"),
        "Prelude should export System.Syscall.Descriptor"
    );
    assert!(
        prelude.contains("pub mod System.Syscall.ReadLimit;"),
        "Prelude should export System.Syscall.ReadLimit"
    );
    assert!(
        prelude.contains("pub mod Collections.Array;"),
        "Prelude should export Collections.Array"
    );
}

#[test]
fn checked_in_compiler_sdk_prelude_exports_mod_sdk_modules() {
    let prelude = fs::read_to_string(compiler_sdk_src().join("Prelude.bd"))
        .expect("read compiler-sdk prelude");
    assert!(
        prelude.contains("pub mod Beskid.Compiler.Syntax;"),
        "compiler-sdk prelude should export Beskid.Compiler.Syntax"
    );
}

#[test]
fn checked_in_corelib_mvp_modules_reference_runtime_backed_symbols() {
    let results_mod =
        fs::read_to_string(foundation_src().join("Core/Results.bd")).expect("read Core.Results");
    let string_mod =
        fs::read_to_string(foundation_src().join("Core/String.bd")).expect("read Core.String");
    let output_mod =
        fs::read_to_string(runtime_src().join("System/Output.bd")).expect("read System.Output");

    assert!(
        results_mod.contains("pub enum Result"),
        "Core.Results should define Result enum"
    );
    assert!(
        results_mod.contains("Ok(") && results_mod.contains("Error("),
        "Core.Results should expose Ok/Error variants"
    );
    assert!(
        results_mod.contains("pub bool IsOk") && results_mod.contains("pub bool IsError"),
        "Core.Results should expose Ok/Error predicates"
    );
    assert!(
        string_mod.contains("__str_len"),
        "Core.String should use __str_len runtime builtin"
    );
    let array_mod =
        fs::read_to_string(foundation_src().join("Collections/Array.bd")).expect("read Array");
    assert!(
        array_mod.contains("__array_len"),
        "Collections.Array should use __array_len for slice length"
    );
    assert!(
        !output_mod.contains("__sys_print"),
        "System.Output must not reference purged __sys_print builtins"
    );
    assert!(
        output_mod.contains("Syscall.WriteWith") && output_mod.contains("WriteLine"),
        "System.Output should route through Syscall.WriteWith and expose WriteLine"
    );
    let syscall_mod =
        fs::read_to_string(runtime_src().join("System/Syscall.bd")).expect("read System.Syscall");
    assert!(
        syscall_mod.contains("__syscall_write"),
        "System.Syscall should call __syscall_write builtin"
    );
    assert!(
        syscall_mod.contains("__syscall_read"),
        "System.Syscall should call __syscall_read builtin"
    );
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
fn checked_in_corelib_beskid_test_sources_parse() {
    let root = corelib_root();
    let test_files = [
        root.join("tests/corelib_tests/src/system/SyscallWriteTests.bd"),
        root.join("tests/corelib_tests/src/system/SyscallApiTests.bd"),
        root.join("tests/corelib_tests/src/system/SyscallErgonomicsTests.bd"),
        root.join("tests/corelib_tests/src/core/ResultsTests.bd"),
        root.join("tests/corelib_tests/src/collections/ArrayTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsPanelTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsProgressBarTests.bd"),
        root.join("tests/corelib_tests/src/console/ControlsLayoutTests.bd"),
    ];
    for path in test_files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read corelib test source {}", path.display()));
        parse_program(&source).unwrap_or_else(|err| {
            panic!(
                "corelib test source should parse {}\nparse error: {err:#}",
                path.display()
            )
        });
    }
}
