//! A valued function whose body can fall off its end is a user error (E1258) in `beskid check`
//! and in `beskid build`; the build never reaches lowering, so it never reports the internal
//! error E2103 for it. Regression for the ICE-sweep mutation `return 0;` -> `0;` in `Main`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const FALLS_OFF: &str = "pub i32 Main()\n{\n    0;\n}\n";

fn beskid(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(arguments)
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("BESKID_HOME", root.join("toolchain-home"))
        .env("BESKID_CORELIB_ROOT", root.join("corelib"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .expect("run beskid")
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[test]
fn check_reports_a_body_that_can_reach_its_end() {
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("Main.bd");
    fs::write(&source, FALLS_OFF).expect("source");
    let output = beskid(root.path(), &["check", "--plain", source.to_str().expect("UTF-8 path")]);
    let report = text(&output);
    assert!(!output.status.success(), "check must fail: {report}");
    assert!(report.contains("E1258"), "check reports E1258: {report}");
    assert!(report.contains("`Main` can reach the end of its body without returning a value"), "{report}");
    assert!(report.contains("`Main` returns `i32`"), "the note names the function and its result: {report}");
    assert!(!report.contains("E2103"), "{report}");

    fs::write(&source, "pub i32 Main()\n{\n    return 0;\n}\n").expect("source");
    let fixed = beskid(root.path(), &["check", "--plain", source.to_str().expect("UTF-8 path")]);
    assert!(fixed.status.success(), "the fixed source passes: {}", text(&fixed));
}

#[test]
fn build_rejects_the_body_before_lowering() {
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("Main.bd");
    fs::write(&source, FALLS_OFF).expect("source");
    let object = root.path().join("Main.o");
    let object = object.to_str().expect("UTF-8 path");
    let output = beskid(
        root.path(),
        &[
            "build",
            "--plain",
            "--offline",
            "--kind",
            "object",
            "--output",
            object,
            "--object-output",
            object,
            source.to_str().expect("UTF-8 path"),
        ],
    );
    let report = text(&output);
    assert!(!output.status.success(), "build must fail: {report}");
    assert!(report.contains("E1258"), "build reports E1258: {report}");
    assert!(!report.contains("E2103"), "build must not reach lowering: {report}");
    assert!(!report.contains("internal compiler error"), "{report}");
}
