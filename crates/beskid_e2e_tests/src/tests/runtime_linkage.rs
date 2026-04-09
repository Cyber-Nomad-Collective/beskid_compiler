use std::time::Duration;

use crate::harness::assertions::{
    assert_exit_code, assert_file_exists, assert_output_contains, assert_success,
};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::run_binary;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn aot_binary_links_runtime_symbols_and_executes_runtime_path() {
    let workspace = E2eWorkspace::from_fixture("smoke_project");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/runtime_linkage");
    let object_output = workspace.join("out/runtime_linkage.o");
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
        object_output.to_str().expect("object path str"),
    ]);
    assert_success(&build, "build runtime-linkage fixture");
    assert_file_exists(&output_binary, "runtime linkage output binary");
    assert_file_exists(&object_output, "runtime linkage object output");

    assert_output_contains(&build, "link:", "build runtime-linkage fixture");

    let run = run_binary(&output_binary, Duration::from_secs(10));
    assert_success(&run, "execute runtime-linkage binary");
    assert_exit_code(&run, 0, "execute runtime-linkage binary");
}
