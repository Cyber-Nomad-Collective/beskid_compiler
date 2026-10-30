//! Inline modules are lexical scopes (`v067-inline-module-scoping`): a constant and a `use`
//! bind in their scope and the modules nested in it, duplicate names are checked per scope, a
//! module never hides a type, and a qualifier that two bindings serve is E1104.
//! Runs against the provisioned Corelib and runtime kit of the test environment.
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn beskid(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
    command
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("BESKID_HOME", root.join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null());
    command
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn check(source: &str) -> (Output, String) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Main.bd");
    std::fs::write(&path, source).unwrap();
    let output = beskid(root.path()).args(["check", "--plain"]).arg(&path).output().unwrap();
    let report = text(&output);
    (output, report)
}

const SCOPES: &str = "use Core.String.Builder;\n\
use Testing.Assert;\n\
\n\
const Limit = 7;\n\
\n\
i64 Size(StringBuilder builder)\n{\n    return 1;\n}\n\
\n\
i64 Helper()\n{\n    return 1;\n}\n\
\n\
mod StringBuilder\n{\n\
    const Limit = 9;\n\
\n\
    test ModuleConstantShadows\n    {\n        Assert.Equal(Limit, 9, \"the module constant\");\n    }\n\
\n\
    test TypeIsNotHidden\n    {\n        StringBuilder builder = Builder.New();\n        Assert.Equal(Size(builder), 1, \"the imported type\");\n    }\n\
}\n\
\n\
mod Topic\n{\n\
    use Core.Random;\n\
\n\
    i64 Helper()\n    {\n        return 2;\n    }\n\
\n\
    test FileConstantVisible\n    {\n        Assert.Equal(Limit, 7, \"the file constant\");\n    }\n\
\n\
    test OwnHelper\n    {\n        Assert.Equal(Helper(), 2, \"the module function\");\n    }\n\
\n\
    test ScopedImport\n    {\n        mut Random rng = Random.New();\n        Assert.True(!Random.IsExhausted(rng), \"the module import\");\n    }\n\
}\n\
\n\
test RootHelper\n{\n    Assert.Equal(Helper(), 1, \"the file function\");\n}\n";

#[test]
fn inline_module_scopes_check_and_run() {
    let (output, report) = check(SCOPES);
    assert!(output.status.success(), "check: {report}");

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Main.bd");
    std::fs::write(&path, SCOPES).unwrap();
    let output = beskid(root.path()).args(["test", "--plain", "--json"]).arg(&path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "test: {}", text(&output));
    let record: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is one JSON record ({error}): {}", text(&output)));
    assert!(record["summary"]["passed"] == 6 && record["summary"]["failed"] == 0, "{record}");
}

#[test]
fn a_use_inside_an_inline_module_does_not_reach_the_file_scope() {
    let (output, report) = check(
        "mod Topic\n{\n    use Core.Random;\n}\n\npub i32 Main()\n{\n    mut Random rng = Random.New();\n    return 0;\n}\n",
    );
    assert_eq!(output.status.code(), Some(1), "check must fail: {report}");
    assert!(report.contains("`Random`"), "{report}");
}

#[test]
fn two_tests_of_one_name_in_one_module_are_one_e1102() {
    let (output, report) = check(
        "mod Topic\n{\n    test Same\n    {\n        return;\n    }\n\n    test Same\n    {\n        return;\n    }\n}\n",
    );
    assert_eq!(output.status.code(), Some(1), "check must fail: {report}");
    assert_eq!(report.matches("E1102").count(), 1, "E1102 once: {report}");
    assert!(!report.contains("E1006"), "no E1006 next to E1102: {report}");
}

#[test]
fn same_names_in_different_scopes_are_no_duplicates() {
    let (output, report) = check(
        "use Core.Random;\n\nbool Helper()\n{\n    return !Random.IsExhausted(Random.New());\n}\n\n\
         mod Alpha\n{\n    use Core.Random;\n\n    bool Helper()\n    {\n        return Random.IsExhausted(Random.New());\n    }\n\n    mod Inner\n    {\n    }\n}\n\n\
         mod Beta\n{\n    mod Inner\n    {\n    }\n}\n\npub i32 Main()\n{\n    bool fresh = Helper();\n    return 0;\n}\n",
    );
    assert!(output.status.success(), "{report}");
}

#[test]
fn a_qualifier_that_names_an_inline_module_and_an_import_is_e1104() {
    let (output, report) = check(
        "use Core.Random;\n\nmod Random\n{\n    pub i64 New()\n    {\n        return 5;\n    }\n}\n\npub i64 Main()\n{\n    return Random.New();\n}\n",
    );
    assert_eq!(output.status.code(), Some(1), "check must fail: {report}");
    assert_eq!(report.matches("E1104").count(), 1, "E1104 once: {report}");
    assert!(report.contains("both the inline module `Random` and the import `Core.Random` declare `New`"), "{report}");
}
