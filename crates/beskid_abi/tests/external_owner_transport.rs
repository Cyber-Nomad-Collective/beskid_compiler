#![cfg(unix)]

use std::{path::Path, process::Command};

#[test]
fn native_owner_wake_closes_each_park_window_and_routes_only_to_its_owner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let output_dir = std::env::temp_dir().join(format!("beskid-owner-transport-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&output_dir).unwrap();
    let executable = output_dir.join("owner_transport");
    let compile = Command::new("cc")
        .args(["-std=c11", "-I"])
        .arg(root.join("include"))
        .arg(root.join("tests/fixtures/external_owner_transport.c"))
        .args(["-lpthread", "-o"])
        .arg(&executable)
        .output()
        .unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = Command::new(executable).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    std::fs::remove_dir_all(output_dir).unwrap();
}
