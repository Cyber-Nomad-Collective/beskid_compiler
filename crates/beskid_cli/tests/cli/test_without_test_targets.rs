//! `beskid test` on a project whose manifest has no Test or Lib target (an App alone, as the
//! console template creates) runs no tests and passes, as a library without `test` items does. An
//! explicitly selected target that is not a Test or Lib target is an error.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn test(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .arg("test")
        .args(args)
        .arg("--plain")
        .env("BESKID_CORELIB_ROOT", root.join("corelib"))
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("OTEL_SDK_DISABLED", "true")
        .current_dir(root)
        .output()
        .expect("run beskid test")
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[test]
fn an_app_project_without_test_targets_passes_with_no_tests() {
    let root = tempfile::tempdir().expect("fixture directory");
    let project = root.path().join("Demo");
    fs::create_dir_all(project.join("Src")).expect("source directory");
    fs::write(
        project.join("Demo.bproj"),
        "Demo {\n  name = \"Demo\"\n  version = \"0.1.0\"\n  root = \"Src\"\n}\n\ntarget \"app\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .expect("manifest");
    fs::write(project.join("Src/Main.bd"), "pub i32 Main()\n{\n    return 0;\n}\n").expect("source");

    let output = test(root.path(), &["--project", "Demo"]);
    assert!(output.status.success(), "an App-only project passes with no tests: {}", text(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("No tests found."), "{}", text(&output));

    let json = test(root.path(), &["--project", "Demo", "--json"]);
    assert!(json.status.success(), "{}", text(&json));
    let record: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&json.stdout).trim()).expect("one JSON result record");
    assert_eq!(record["event"], "result");
    assert_eq!(record["status"], "success");
    assert_eq!(record["summary"]["passed"], 0);

    let selected = test(root.path(), &["--project", "Demo", "--target", "app"]);
    assert!(!selected.status.success(), "an App target is not a test target: {}", text(&selected));
    assert!(text(&selected).contains("target `app` is not a Test or Lib target"), "{}", text(&selected));
}
