//! Public toolchain operations bind to the running executable rather than an unrelated store.
use std::process::Command;
#[test]
fn v06_toolchain_status_describes_running_executable_without_provisioning() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["toolchain", "status"])
        .env("BESKID_HOME", root.path().join("unrelated-store"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("owner")
            && text.contains(std::fs::canonicalize(env!("CARGO_BIN_EXE_beskid_cli")).unwrap().to_str().unwrap()),
        "{text}"
    );
    assert!(!root.path().join("unrelated-store").exists());
    assert!(!root.path().join("config").exists());
}

#[test]
fn v06_toolchain_update_rejects_unowned_running_binary_before_release_resolution() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["toolchain", "update"])
        .env("BESKID_HOME", root.path().join("unrelated-store"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env_remove("BESKID_RELEASE_MANIFEST_URL")
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(text.contains("owner") || text.contains("unmanaged"), "{text}");
    assert!(
        !text.contains("set beskid_release_manifest_url"),
        "ownership must be checked before download configuration: {text}"
    );
    assert!(!root.path().join("unrelated-store").exists());
}
