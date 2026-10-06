//! `beskid check` must use its reported semantic severity as process status.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn semantic_error_fails_but_clean_source_succeeds() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let root = std::env::temp_dir().join(format!("beskid_analyze_exit_{}_{}", std::process::id(), nonce));
    fs::create_dir_all(&root).expect("create fixture");

    let invalid = root.join("Invalid.bd");
    fs::write(&invalid, "pub i32 Main() { return UnknownValue; }\n").expect("write invalid source");
    let failed = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["check", "--plain"])
        .arg(&invalid)
        .env("BESKID_CORELIB_ROOT", root.join("corelib"))
        .output()
        .expect("analyze invalid source");
    let failed_text = format!("{}{}", String::from_utf8_lossy(&failed.stdout), String::from_utf8_lossy(&failed.stderr));
    assert!(failed_text.contains("error"), "expected semantic error: {failed_text}");
    assert!(!failed.status.success(), "semantic errors must fail: {failed_text}");
    assert_eq!(
        failed_text.matches("unknown value `UnknownValue`").count(),
        1,
        "semantic diagnostic should be rendered once: {failed_text}"
    );

    let valid = root.join("Valid.bd");
    fs::write(&valid, "pub i32 Main() { return 0; }\n").expect("write valid source");
    let passed = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["check", "--plain"])
        .arg(&valid)
        .env("BESKID_CORELIB_ROOT", root.join("corelib"))
        .output()
        .expect("analyze valid source");
    let passed_text = format!("{}{}", String::from_utf8_lossy(&passed.stdout), String::from_utf8_lossy(&passed.stderr));
    assert!(passed.status.success(), "valid source must pass: {passed_text}");

    fs::remove_dir_all(&root).expect("remove owned fixture");
}
