use std::fs;

use beskid_analysis::projects::build_compile_plan;
use beskid_analysis::services::{parse_program, resolve_input};
use beskid_codegen::lower_source;

use super::{expected_stdlib_files, stdlib_root};

#[test]
fn checked_in_stdlib_template_builds_compile_plan() {
    let manifest_path = stdlib_root().join("Project.proj");
    let plan =
        build_compile_plan(&manifest_path, Some("StdLib")).expect("stdlib plan should build");
    let expected_root = stdlib_root()
        .join("src")
        .canonicalize()
        .expect("canonical stdlib source root");
    let actual_root = plan
        .source_root
        .canonicalize()
        .expect("canonical compile-plan source root");

    assert_eq!(plan.target.name, "StdLib");
    assert_eq!(actual_root, expected_root);
    assert!(plan.source_root.join("Prelude.bd").is_file());
}

#[test]
fn checked_in_stdlib_sources_parse_as_beskid_programs() {
    let root = stdlib_root().join("src");

    let prelude_path = root.join("Prelude.bd");
    let prelude = fs::read_to_string(&prelude_path).expect("read prelude");
    parse_program(&prelude).expect("prelude should parse");

    for relative in expected_stdlib_files() {
        let path = root.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read stdlib source {}", path.display()));
        parse_program(&source).unwrap_or_else(|err| {
            panic!(
                "stdlib source should parse {}\nparse error: {err:?}",
                path.display()
            )
        });
    }
}

#[test]
fn checked_in_stdlib_prelude_lowers_to_codegen_artifact() {
    let project = stdlib_root();
    let resolved = resolve_input(None, Some(&project), Some("StdLib"), None, false, false)
        .expect("resolve stdlib project input");

    let _lowered = lower_source(&resolved.source_path, &resolved.source, true)
        .expect("lower stdlib prelude should succeed");
}

#[test]
fn checked_in_stdlib_prelude_exports_mvp_modules() {
    let root = stdlib_root().join("src");
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
        prelude.contains("pub mod System.IO;"),
        "Prelude should export System.IO"
    );
}

#[test]
fn checked_in_stdlib_mvp_modules_reference_runtime_backed_symbols() {
    let root = stdlib_root().join("src");
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
        io_mod.contains("__sys_print("),
        "System.IO.Print should use __sys_print runtime builtin"
    );
    assert!(
        io_mod.contains("__sys_println("),
        "System.IO.Println should use __sys_println runtime builtin"
    );
}
