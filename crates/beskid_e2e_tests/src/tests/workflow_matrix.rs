use std::fs;
use std::time::Duration;

use crate::harness::assertions::{
    assert_exit_code, assert_failure, assert_file_exists, assert_output_contains, assert_success,
};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::run_binary;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn lock_then_build_locked_succeeds_for_workspace_member() {
    let workspace = E2eWorkspace::from_fixture("deps_workspace");
    let workspace_manifest = workspace.join("Workspace.proj");
    let app_manifest = workspace.join("app/Project.proj");
    let output_binary = workspace.join("out/workflow_locked_app");
    let cli = BeskidCliInvoker::new();

    let lock = cli.run([
        "lock",
        "--project",
        workspace_manifest.to_str().expect("workspace path str"),
        "--workspace-member",
        "app",
    ]);
    assert_success(&lock, "lock workflow matrix app");
    assert_file_exists(&workspace.join("app/Project.lock"), "app lockfile");

    let build_locked = cli.run([
        "build",
        "--project",
        app_manifest.to_str().expect("app path str"),
        "--target",
        "App",
        "--locked",
        "--output",
        output_binary.to_str().expect("output path str"),
    ]);
    assert_success(&build_locked, "locked build workflow matrix app");
    assert_file_exists(&output_binary, "locked build output binary");

    let run = run_binary(&output_binary, Duration::from_secs(10));
    assert_success(&run, "run locked build workflow matrix app");
    assert_exit_code(&run, 0, "run locked build workflow matrix app");
}

#[test]
fn fetch_locked_fails_when_lockfile_is_removed() {
    let workspace = E2eWorkspace::from_fixture("deps_workspace");
    let workspace_manifest = workspace.join("Workspace.proj");
    let lock_path = workspace.join("app/Project.lock");
    let cli = BeskidCliInvoker::new();

    let lock = cli.run([
        "lock",
        "--project",
        workspace_manifest.to_str().expect("workspace path str"),
        "--workspace-member",
        "app",
    ]);
    assert_success(&lock, "initial lock for fetch-locked failure case");
    assert_file_exists(&lock_path, "lockfile before deletion");
    fs::remove_file(&lock_path).expect("delete lockfile");

    let fetch_locked = cli.run([
        "fetch",
        "--project",
        workspace_manifest.to_str().expect("workspace path str"),
        "--workspace-member",
        "app",
        "--locked",
    ]);
    assert_failure(&fetch_locked, "fetch locked without lockfile");
    assert_output_contains(&fetch_locked, "lock", "fetch locked without lockfile");
}
