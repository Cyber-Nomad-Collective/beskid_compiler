use std::time::Duration;
use std::{fs, path::Path};

use crate::harness::assertions::{
    assert_exit_code, assert_file_exists, assert_output_contains, assert_success,
};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::run_binary;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn fetch_lock_update_then_build_and_run_project_with_path_dependency() {
    let workspace = E2eWorkspace::from_fixture("deps_workspace");
    let workspace_manifest = workspace.join("Workspace.proj");
    let app_manifest = workspace.join("app/Project.proj");
    let output_binary = workspace.join("out/deps_app");

    let cli = BeskidCliInvoker::new();

    let fetch = cli.run([
        "fetch",
        "--project",
        workspace_manifest.to_str().expect("workspace path str"),
        "--workspace-member",
        "app",
    ]);
    assert_success(&fetch, "fetch dependency workflow fixture");
    assert_output_contains(
        &fetch,
        "Dependencies resolved and materialized",
        "fetch dependency workflow fixture",
    );

    let update = cli.run([
        "update",
        "--project",
        workspace_manifest.to_str().expect("workspace path str"),
        "--workspace-member",
        "app",
    ]);
    assert_success(&update, "update dependency workflow fixture");
    assert_output_contains(
        &update,
        "Dependency lock and materialized workspace updated",
        "update dependency workflow fixture",
    );

    let lock = cli.run([
        "lock",
        "--project",
        workspace_manifest.to_str().expect("workspace path str"),
        "--workspace-member",
        "app",
    ]);
    assert_success(&lock, "lock dependency workflow fixture");
    assert_output_contains(
        &lock,
        "Project.lock synchronized",
        "lock dependency workflow fixture",
    );

    assert_file_exists(&workspace.join("app/Project.lock"), "project lockfile");
    let deps_src = workspace.join("app/obj/beskid/deps/src");
    let manifest_count = count_project_manifests(&deps_src);
    assert!(
        manifest_count >= 2,
        "expected at least 2 materialized dependency manifests, found {manifest_count} under {}",
        deps_src.display()
    );

    let build = cli.run([
        "build",
        "--project",
        app_manifest.to_str().expect("app path str"),
        "--target",
        "App",
        "--output",
        output_binary.to_str().expect("output path str"),
    ]);
    assert_success(&build, "build dependency workflow fixture");
    assert_file_exists(&output_binary, "dependency workflow output binary");

    let run = run_binary(&output_binary, Duration::from_secs(10));
    assert_success(&run, "execute dependency workflow binary");
    assert_exit_code(&run, 0, "execute dependency workflow binary");
}

fn count_project_manifests(root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };

    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += count_project_manifests(&path);
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "Project.proj")
        {
            count += 1;
        }
    }

    count
}
