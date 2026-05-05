use std::fs;

use beskid_analysis::projects::build_compile_plan;
use beskid_analysis::services::{analyze_file_in_project, parse_program, resolve_input};
use beskid_analysis::Severity;
use beskid_codegen::lower_source;

use crate::projects::std_dependency_env_lock;

use super::{corelib_root, expected_corelib_files};

#[test]
fn checked_in_corelib_template_builds_compile_plan() {
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
}

#[test]
fn checked_in_corelib_sources_parse_as_beskid_programs() {
    let root = corelib_root().join("src");

    for relative in expected_corelib_files() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read corelib source {}", path.display()));
        parse_program(&source).unwrap_or_else(|err| {
            panic!(
                "corelib source should parse {}\nparse error: {err:?}",
                path.display()
            )
        });
    }
}

#[test]
fn checked_in_corelib_syscall_file_does_not_report_module_resolution_false_positives() {
    let diagnostics = analyze_file_in_project(&corelib_root().join("src/System/Syscall.bd"))
        .expect("analyze corelib syscall source");

    assert!(
        diagnostics
            .iter()
            .all(|diag| !matches!(diag.code.as_deref(), Some("E1005") | Some("E1105"))),
        "corelib syscall file should not emit E1005/E1105 false positives: {diagnostics:#?}"
    );
}

#[test]
fn checked_in_corelib_sources_do_not_emit_error_diagnostics_in_project_context() {
    let root = corelib_root().join("src");

    for relative in expected_corelib_files() {
        let path = root.join(relative);
        let diagnostics =
            analyze_file_in_project(&path).unwrap_or_else(|_| panic!("analyze {}", path.display()));
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
}

#[test]
fn corelib_mvp_fixture_entry_does_not_emit_module_resolution_false_positives() {
    let fixture_main = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
    let diagnostics = analyze_file_in_project(&fixture_main).expect("analyze corelib_mvp fixture");

    assert!(
        diagnostics
            .iter()
            .all(|diag| !matches!(diag.code.as_deref(), Some("E1105") | Some("E1108"))),
        "corelib_mvp fixture should not emit module-path false positives: {diagnostics:#?}"
    );
}

#[test]
fn checked_in_corelib_prelude_lowers_to_codegen_artifact() {
    let _env_guard = std_dependency_env_lock();
    let project = corelib_root();
    let resolved = resolve_input(None, Some(&project), Some("CoreLib"), None, false, false)
        .expect("resolve corelib project input");

    let _lowered = lower_source(&resolved.source_path, &resolved.source, true)
        .expect("lower corelib prelude should succeed");
}

#[test]
fn checked_in_corelib_prelude_exports_mvp_modules() {
    let root = corelib_root().join("src");
    let prelude = fs::read_to_string(root.join("Prelude.bd")).expect("read prelude");

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
        prelude.contains("pub mod System.IO;"),
        "Prelude should export System.IO"
    );
    assert!(
        prelude.contains("pub mod System.Syscall;"),
        "Prelude should export System.Syscall"
    );
}

#[test]
fn checked_in_corelib_mvp_modules_reference_runtime_backed_symbols() {
    let root = corelib_root().join("src");
    let results_mod = fs::read_to_string(root.join("Core/Results.bd")).expect("read Core.Results");
    let string_mod = fs::read_to_string(root.join("Core/String.bd")).expect("read Core.String");
    let io_mod = fs::read_to_string(root.join("System/IO.bd")).expect("read System.IO");

    assert!(
        results_mod.contains("pub enum Result"),
        "Core.Results should define Result enum"
    );
    assert!(
        results_mod.contains("Ok(") && results_mod.contains("Error("),
        "Core.Results should expose Ok/Error variants"
    );
    assert!(
        string_mod.contains("__str_len"),
        "Core.String should use __str_len runtime builtin"
    );
    assert!(
        !io_mod.contains("__sys_print"),
        "System.IO must not reference purged __sys_print builtins"
    );
    assert!(
        io_mod.contains("Syscall.Write") && io_mod.contains("PrintLine"),
        "System.IO should route output through Syscall.Write and expose PrintLine"
    );
    let syscall_mod =
        fs::read_to_string(root.join("System/Syscall.bd")).expect("read System.Syscall");
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
fn checked_in_corelib_beskid_test_sources_parse() {
    let root = corelib_root();
    let test_files = [
        root.join("tests/corelib_tests/src/system/SyscallWriteTests.bd"),
        root.join("tests/corelib_tests/src/system/SyscallApiTests.bd"),
    ];
    for path in test_files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read corelib test source {}", path.display()));
        parse_program(&source).unwrap_or_else(|err| {
            panic!(
                "corelib test source should parse {}\nparse error: {err:?}",
                path.display()
            )
        });
    }
}
