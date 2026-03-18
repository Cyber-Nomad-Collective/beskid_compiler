use std::fs;

use beskid_analysis::projects::build_compile_plan;
use beskid_analysis::services::{parse_program, resolve_input};
use beskid_codegen::lower_source;

use super::{expected_stdlib_files, stdlib_root};

#[test]
fn checked_in_stdlib_template_builds_compile_plan() {
    let manifest_path = stdlib_root().join("Project.proj");
    let plan = build_compile_plan(&manifest_path, Some("StdLib")).expect("stdlib plan should build");
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
fn checked_in_stdlib_query_contracts_match_parser_supported_baseline() {
    let root = stdlib_root().join("src");
    let contracts = fs::read_to_string(root.join("Query/Contracts.bd")).expect("read query contracts");
    let operators = fs::read_to_string(root.join("Query/Operators.bd")).expect("read query operators");

    assert!(
        contracts.contains("pub enum Option"),
        "Query.Contracts should define Option"
    );
    assert!(
        contracts.contains("pub contract Iterator"),
        "Query.Contracts should define Iterator"
    );
    assert!(
        operators.contains("Query.Contracts.Option first"),
        "QueryState should track first-value option"
    );
    assert!(
        operators.contains("Query.Contracts.HasValue(state.first)"),
        "First() should consult tracked first-value option"
    );
}

#[test]
fn checked_in_stdlib_system_baseline_exports_environment_and_process_contracts() {
    let root = stdlib_root().join("src");
    let environment =
        fs::read_to_string(root.join("System/Environment.bd")).expect("read system environment");
    let process = fs::read_to_string(root.join("System/Process.bd")).expect("read system process");

    assert!(
        environment.contains("pub enum EnvironmentError"),
        "Environment should expose typed error enum"
    );
    assert!(
        environment.contains("pub Core.Results.Result<string, EnvironmentError> Get("),
        "Environment.Get should return typed Result"
    );
    assert!(
        environment.contains("pub Query.Contracts.Option TryGet("),
        "Environment.TryGet should return Option<string>"
    );
    assert!(
        process.contains("pub i32 Id()"),
        "Process should expose Id baseline"
    );
    assert!(
        process.contains("pub unit Exit(i32 code)"),
        "Process should expose Exit baseline"
    );
}
