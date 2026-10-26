//! Corelib packages build standalone: `beskid build` inside a package directory of a Corelib
//! copy emits the package library. The compiler SDK package wraps host callbacks that run only
//! inside a prepared native Mod invocation; its library output leaves those wrappers out
//! instead of failing the build with an internal error (E2103).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn corelib_checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib").canonicalize().expect("compiler corelib checkout")
}

/// Copy the Corelib sources without build outputs, so the package builds in a fresh tree.
fn copy_corelib(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        !(entry.file_type().is_dir() && matches!(name.as_ref(), "obj" | ".git" | ".beskid" | "target"))
    }) {
        let entry = entry.expect("walk Corelib checkout");
        let relative = entry.path().strip_prefix(from).expect("Corelib-relative path");
        let destination = to.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).expect("Corelib copy directory");
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), &destination).expect("Corelib copy file");
        }
    }
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn build_package(corelib: &Path, package: &str) -> Output {
    let project = corelib.join("packages").join(package);
    let artifact = project.join(format!("{package}.o"));
    Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["build", "--plain", "--offline", "--kind", "object", "--output"])
        .arg(&artifact)
        .arg("--object-output")
        .arg(&artifact)
        .env("BESKID_CORELIB_ROOT", corelib)
        .current_dir(&project)
        .output()
        .expect("run beskid build")
}

#[test]
fn compiler_sdk_and_console_packages_build_standalone() {
    let root = tempfile::tempdir().expect("temporary Corelib root");
    let corelib = root.path().join("corelib");
    copy_corelib(&corelib_checkout(), &corelib);
    for package in ["compiler-sdk", "console"] {
        let output = build_package(&corelib, package);
        let report = text(&output);
        assert!(output.status.success(), "`beskid build` of Corelib package {package} failed:\n{report}");
        assert!(!report.contains("internal compiler error"), "{package}: internal error in a passing build:\n{report}");
    }
}
