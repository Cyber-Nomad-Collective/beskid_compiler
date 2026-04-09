use crate::harness::assertions::{assert_failure, assert_output_contains};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn fetch_fails_when_manifest_is_missing() {
    let empty = tempfile::Builder::new()
        .prefix("beskid_e2e_missing_manifest_")
        .tempdir()
        .expect("create temp dir");
    let project_path = empty.path().join("Project.proj");
    let cli = BeskidCliInvoker::new();

    let result = cli.run([
        "fetch",
        "--project",
        project_path.to_str().expect("project path str"),
    ]);
    assert_failure(&result, "fetch missing manifest");
    assert_output_contains(&result, "Project.proj", "fetch missing manifest");
}

#[test]
fn build_fails_for_unknown_target() {
    let workspace = E2eWorkspace::from_fixture("smoke_project");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/unknown_target");
    let cli = BeskidCliInvoker::new();

    let result = cli.run([
        "build",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "MissingTarget",
        "--output",
        output_binary.to_str().expect("output path str"),
    ]);
    assert_failure(&result, "build unknown target fixture");
    assert_output_contains(&result, "target", "build unknown target fixture");
}

#[test]
fn build_locked_mode_fails_without_lockfile() {
    let workspace = E2eWorkspace::from_fixture("smoke_project");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/locked_build");
    let cli = BeskidCliInvoker::new();

    let result = cli.run([
        "build",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
        "--locked",
        "--output",
        output_binary.to_str().expect("output path str"),
    ]);
    assert_failure(&result, "build locked mode without lockfile");
    assert_output_contains(&result, "lock", "build locked mode without lockfile");
}

#[test]
fn build_reports_linker_unavailable_with_invalid_cc() {
    let workspace = E2eWorkspace::from_fixture("smoke_project");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/link_fail");
    let cli = BeskidCliInvoker::new();

    let mut command = cli.command([
        "build",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
        "--output",
        output_binary.to_str().expect("output path str"),
    ]);
    command.env("CC", "definitely-not-a-c-compiler");
    let result = command.output().expect("run build with invalid CC");

    assert_failure(&result, "build invalid linker toolchain");
    assert_output_contains(
        &result,
        "Linker tool not available",
        "build invalid linker toolchain",
    );
}
