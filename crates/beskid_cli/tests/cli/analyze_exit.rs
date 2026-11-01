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

/// A project check covers every manifest target: an error reachable only from a non-default target (a per-suite
/// test entry) must fail the check, while selecting a clean target still passes.
#[test]
fn project_check_covers_every_manifest_target() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join("Src")).expect("source directory");
    fs::write(root.path().join("Src/Clean.bd"), "pub i32 Main() { return 0; }\n").expect("clean source");
    fs::write(root.path().join("Src/Broken.bd"), "pub i32 Broken() { return UnknownValue; }\n").expect("broken source");
    let manifest = root.path().join("Multi.bproj");
    fs::write(
        &manifest,
        "Multi { name = \"Multi\" version = \"0.1.0\" root = \"Src\" }\n\
         target \"Clean\" { kind = Lib entry = \"Clean.bd\" }\n\
         target \"Broken\" { kind = Lib entry = \"Broken.bd\" }\n",
    )
    .expect("manifest");
    let check = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
            .args(["check", "--plain", "--project"])
            .arg(&manifest)
            .args(extra)
            .env("BESKID_CONFIG_DIR", root.path().join("config"))
            .env("BESKID_HOME", root.path().join("toolchain-home"))
            .env("OTEL_SDK_DISABLED", "true")
            .output()
            .expect("run check");
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        (output.status.success(), text)
    };
    let (all_ok, all_text) = check(&[]);
    assert!(!all_ok, "an error in a non-default target must fail the project check: {all_text}");
    assert!(all_text.contains("unknown value `UnknownValue`"), "the broken target is reported: {all_text}");
    let (clean_ok, clean_text) = check(&["--target", "Clean"]);
    assert!(clean_ok, "an explicitly selected clean target passes: {clean_text}");
}

/// A schema-only `type = Bsol` project has no Beskid source: `beskid check` validates its schema
/// exports and succeeds, and a broken export fails the check.
#[test]
fn bsol_project_check_validates_its_schema_exports() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/pest-gen-schema/pest_gen_schema.bproj");
    let root = tempfile::tempdir().expect("fixture directory");
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["check", "--plain", "--project"])
        .arg(&manifest)
        .env("BESKID_CORELIB_ROOT", root.path().join("corelib"))
        .output()
        .expect("check the shipped schema package");
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "a valid Bsol project checks clean: {text}");
    assert!(!text.contains("E1888"), "check must not treat a Bsol project as Beskid source: {text}");
    assert!(text.contains("validated exported schema profile `pestGenConfig.v1`"), "{text}");

    let broken = root.path().join("broken");
    fs::create_dir_all(broken.join("schemas")).expect("schema directory");
    fs::write(
        broken.join("broken.bproj"),
        "broken {\n  name = \"broken\"\n  version = \"0.1.0\"\n  type = Bsol\n  schemas {\n    export \"missing\" {\n      profile = \"missing.v1\"\n      path = \"schemas/missing.v1.bsol\"\n    }\n  }\n}\n",
    )
    .expect("broken manifest");
    let failed = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["check", "--plain", "--project"])
        .arg(broken.join("broken.bproj"))
        .env("BESKID_CORELIB_ROOT", root.path().join("corelib"))
        .output()
        .expect("check a Bsol project with a missing export");
    let failed_text = format!("{}{}", String::from_utf8_lossy(&failed.stdout), String::from_utf8_lossy(&failed.stderr));
    assert!(!failed.status.success(), "a missing schema export fails the check: {failed_text}");
}
