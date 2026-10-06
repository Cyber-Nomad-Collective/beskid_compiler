#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded};
use std::{path::Path, process::Command, time::Duration};

#[test]
fn v06_child_transport_binary_pipes_argv_and_bounded_reap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join(executable_name("child_transport"));
    let mut compiler = native_c_compiler();
    compiler
        .args(["-Wall", "-Wextra", "-Werror", "-I"])
        .arg(root.join("include"))
        .arg(root.join("tests/fixtures/process_transport_v06.c"));
    if !cfg!(windows) {
        compiler.arg("-lpthread");
    }
    compiler.arg("-o").arg(&executable);
    let compiled = run_bounded("child transport compilation", &mut compiler, Duration::from_secs(60));
    assert!(compiled.status.success(), "C fixture compile failed: {}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_bounded("child transport execution", &mut Command::new(&executable), Duration::from_secs(15));
    assert!(
        output.status.success(),
        "child fixture failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("binary=exact argv=exact stderr=drained reap=bounded"));
}
