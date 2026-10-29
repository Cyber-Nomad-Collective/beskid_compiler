//! Target selection without `--target`. `build` and `run` never select a Test target: `run` takes
//! the App target and `build` the App target, or the Lib target when there is no App target.
//! `test` runs every Test target in manifest order with a combined summary, or the Lib target when
//! there is no Test target. Two candidates of the same kind fail with an error that names them.
//! Runs against the provisioned Corelib and runtime kit of the test environment.
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LIB: &str = "pub i32 Answer()\n{\n    return 42;\n}\n";
const LIB_TESTS: &str =
    "use Lib;\nuse Testing.Assert;\n\ntest AnswerIsFortyTwo\n{\n    Assert.True(Lib.Answer() == 42, \"answer\");\n}\n";
const MAIN: &str = "pub i32 Main()\n{\n    return 7;\n}\n";

/// A project `Demo` in a fresh directory with `targets` (`(name, kind, entry)`, in manifest order)
/// and `sources` (`(file under Src, text)`).
fn project(targets: &[(&str, &str, &str)], sources: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("fixture directory");
    let dir = root.path().join("Demo");
    std::fs::create_dir_all(dir.join("Src")).expect("source directory");
    let mut manifest = String::from("Demo {\n  name = \"Demo\"\n  version = \"0.1.0\"\n  root = \"Src\"\n}\n");
    for (name, kind, entry) in targets {
        manifest.push_str(&format!("\ntarget \"{name}\" {{\n  kind = {kind}\n  entry = \"{entry}\"\n}}\n"));
    }
    std::fs::write(dir.join("Demo.bproj"), manifest).expect("manifest");
    for (file, text) in sources {
        std::fs::write(dir.join("Src").join(file), text).expect("source");
    }
    (root, dir)
}

/// Runs `beskid <args> --plain` in the project directory, with no `--project` or `--target`.
fn beskid(root: &Path, dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(args)
        .arg("--plain")
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("BESKID_HOME", root.join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .current_dir(dir)
        .output()
        .expect("run beskid")
}

fn text(output: &Output) -> String {
    format!("stdout: {}\nstderr: {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The file stem of the artifact a `build --json` reports, which is the built target's name.
fn built_target(output: &Output) -> String {
    assert!(output.status.success(), "build: {}", text(output));
    let record: serde_json::Value = serde_json::from_str(stdout(output).trim().lines().last().expect("result record"))
        .unwrap_or_else(|error| panic!("JSON result record ({error}): {}", text(output)));
    let path = PathBuf::from(record["output"].as_str().unwrap_or_else(|| panic!("output path: {record}")));
    assert!(path.is_file(), "artifact {} exists", path.display());
    path.file_stem().and_then(|stem| stem.to_str()).expect("artifact stem").to_string()
}

#[test]
fn a_lib_project_with_tests_builds_the_lib_target_and_tests_the_test_target() {
    let (root, dir) = project(
        &[("lib", "Lib", "Lib.bd"), ("tests", "Test", "LibTests.bd")],
        &[("Lib.bd", LIB), ("LibTests.bd", LIB_TESTS)],
    );

    let build = beskid(root.path(), &dir, &["build", "--json"]);
    let shared_library = format!("{}lib", std::env::consts::DLL_PREFIX);
    assert_eq!(built_target(&build), shared_library, "the Lib target is built: {}", text(&build));
    assert!(!text(&build).contains("Missing entrypoint"), "{}", text(&build));

    let test = beskid(root.path(), &dir, &["test"]);
    assert!(test.status.success(), "{}", text(&test));
    assert!(stdout(&test).contains("Result: passed=1, failed=0"), "the Test target runs: {}", text(&test));
    assert!(!stdout(&test).contains("No tests found."), "{}", text(&test));
}

#[test]
fn an_app_project_with_tests_runs_and_builds_the_app_target_and_tests_the_test_target() {
    let (root, dir) = project(
        &[("tests", "Test", "LibTests.bd"), ("app", "App", "Main.bd"), ("lib", "Lib", "Lib.bd")],
        &[("Main.bd", MAIN), ("Lib.bd", LIB), ("LibTests.bd", LIB_TESTS)],
    );

    let run = beskid(root.path(), &dir, &["run"]);
    assert_eq!(run.status.code(), Some(7), "the App target runs: {}", text(&run));

    let build = beskid(root.path(), &dir, &["build", "--json"]);
    assert_eq!(built_target(&build), "app", "{}", text(&build));

    let test = beskid(root.path(), &dir, &["test"]);
    assert!(test.status.success(), "{}", text(&test));
    assert!(text(&test).contains("AnswerIsFortyTwo"), "the Test target runs: {}", text(&test));
    assert!(stdout(&test).contains("Result: passed=1, failed=0"), "{}", text(&test));
}

#[test]
fn test_runs_every_test_target_in_manifest_order_with_a_combined_summary() {
    let failing = "use Testing.Assert;\n\ntest Breaks\n{\n    Assert.Fail(\"second target fails\");\n}\n";
    let (root, dir) = project(
        &[("lib", "Lib", "Lib.bd"), ("unit", "Test", "LibTests.bd"), ("more", "Test", "More.bd")],
        &[("Lib.bd", LIB), ("LibTests.bd", LIB_TESTS), ("More.bd", "test AlsoRuns\n{\n    return;\n}\n")],
    );

    let test = beskid(root.path(), &dir, &["test"]);
    assert!(test.status.success(), "{}", text(&test));
    let out = stdout(&test);
    let unit = out.find("Target `unit`").unwrap_or_else(|| panic!("unit runs: {}", text(&test)));
    let more = out.find("Target `more`").unwrap_or_else(|| panic!("more runs: {}", text(&test)));
    assert!(unit < more, "manifest order: {}", text(&test));
    assert!(text(&test).contains("AnswerIsFortyTwo") && text(&test).contains("AlsoRuns"), "{}", text(&test));
    assert!(
        out.contains("Total: targets=2, failed_targets=0, passed=2, failed=0"),
        "combined summary: {}",
        text(&test)
    );

    let json = beskid(root.path(), &dir, &["test", "--json"]);
    assert!(json.status.success(), "{}", text(&json));
    let record: serde_json::Value = serde_json::from_str(stdout(&json).trim())
        .unwrap_or_else(|error| panic!("one record ({error}): {}", text(&json)));
    assert_eq!(record["event"], "result");
    assert_eq!(record["status"], "success");
    assert_eq!(record["summary"]["passed"], 2, "{record}");
    let targets = record["targets"].as_array().expect("targets");
    assert_eq!(targets.iter().map(|target| target["target"].as_str().unwrap()).collect::<Vec<_>>(), ["unit", "more"]);

    std::fs::write(dir.join("Src/More.bd"), failing).expect("failing source");
    let failed = beskid(root.path(), &dir, &["test"]);
    assert!(!failed.status.success(), "a failed target fails the run: {}", text(&failed));
    assert!(stdout(&failed).contains("Total: targets=2, failed_targets=1, passed=1, failed=1"), "{}", text(&failed));
    assert!(text(&failed).contains("1 of 2 test targets failed: `more`"), "{}", text(&failed));
}

#[test]
fn two_app_targets_fail_build_and_run_with_an_error_that_names_them() {
    let (root, dir) = project(&[("one", "App", "Main.bd"), ("two", "App", "Main.bd")], &[("Main.bd", MAIN)]);
    for command in ["build", "run"] {
        let output = beskid(root.path(), &dir, &[command]);
        assert!(!output.status.success(), "{command}: {}", text(&output));
        assert!(
            text(&output).contains(&format!(
                "`beskid {command}` found 2 App targets (`one` (App), `two` (App)) and cannot choose one; pass `--target <name>`"
            )),
            "{command}: {}",
            text(&output)
        );
    }
    let selected = beskid(root.path(), &dir, &["run", "--target", "two"]);
    assert_eq!(selected.status.code(), Some(7), "`--target` selects one: {}", text(&selected));
}

#[test]
fn without_a_test_target_test_runs_the_lib_target_and_build_never_selects_a_test_target() {
    let lib_with_test = "pub i32 Answer()\n{\n    return 42;\n}\n\ntest LibSelfTest\n{\n    return;\n}\n";
    let (root, dir) = project(
        &[("app", "App", "Main.bd"), ("lib", "Lib", "Lib.bd")],
        &[("Main.bd", MAIN), ("Lib.bd", lib_with_test)],
    );
    let test = beskid(root.path(), &dir, &["test"]);
    assert!(test.status.success(), "{}", text(&test));
    assert!(text(&test).contains("LibSelfTest"), "the Lib target is tested: {}", text(&test));

    let (root, dir) =
        project(&[("tests", "Test", "LibTests.bd")], &[("LibTests.bd", "test Only\n{\n    return;\n}\n")]);
    let build = beskid(root.path(), &dir, &["build"]);
    assert!(!build.status.success(), "{}", text(&build));
    assert!(text(&build).contains("`beskid build` needs an App or Lib target"), "{}", text(&build));
    assert!(text(&build).contains("`tests` (Test)"), "{}", text(&build));
    let run = beskid(root.path(), &dir, &["run"]);
    assert!(!run.status.success(), "{}", text(&run));
    assert!(text(&run).contains("`beskid run` needs an App target"), "{}", text(&run));
}
