use std::path::Path;
use std::process::Command;

#[test]
fn vendored_dependencies_are_not_workspace_test_targets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()
        .expect("inspect Cargo workspace members");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).expect("Cargo metadata JSON");
    let members = metadata["workspace_members"].as_array().expect("Cargo workspace members");
    for vendor in ["/vendor/ratkit#", "/vendor/cargo-cross-patched#"] {
        assert!(
            !members.iter().any(|member| member.as_str().is_some_and(|id| id.contains(vendor))),
            "vendored dependency {vendor} must not pull its dev dependencies into --workspace --all-targets"
        );
    }
    assert!(
        members.iter().any(|member| member.as_str().is_some_and(|id| id.contains("/vendor/salsa-patched#"))),
        "the patched Salsa package remains an intentional workspace member"
    );
}
