use std::time::Duration;

use crate::harness::assertions::{
    assert_exit_code, assert_file_exists, assert_output_contains, assert_success,
};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::run_binary;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn aot_build_and_execute_smoke_fixture() {
    let workspace = E2eWorkspace::from_fixture("smoke_project");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/smoke_app");
    let object_output = workspace.join("out/smoke_app.o");

    let cli = BeskidCliInvoker::new();
    let build = cli.run([
        "build",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
        "--output",
        output_binary.to_str().expect("output path str"),
        "--object-output",
        object_output.to_str().expect("object output path str"),
    ]);
    assert_success(&build, "build smoke fixture");
    assert_output_contains(&build, "object:", "build smoke fixture");
    assert_output_contains(&build, "output:", "build smoke fixture");

    assert_file_exists(&output_binary, "smoke build output");
    assert_file_exists(&object_output, "smoke object output");

    let run = run_binary(&output_binary, Duration::from_secs(10));
    assert_success(&run, "execute smoke binary");
    assert_exit_code(&run, 0, "execute smoke binary");
}
