use crate::harness::assertions::{assert_file_exists, assert_output_contains, assert_success};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::nm_contains_symbol;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn runtime_calls_fixture_jit_runs_and_aot_contains_runtime_symbols() {
    let workspace = E2eWorkspace::from_fixture("runtime_calls");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/runtime_calls");
    let object_output = workspace.join("out/runtime_calls.o");
    let cli = BeskidCliInvoker::new();

    let jit_run = cli.run([
        "run",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
    ]);
    assert_success(&jit_run, "run runtime-calls fixture through JIT");
    assert_output_contains(&jit_run, "0", "run runtime-calls fixture through JIT");

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
    assert_success(&build, "build runtime-calls fixture");
    assert_file_exists(&output_binary, "runtime-calls output binary");
    assert_file_exists(&object_output, "runtime-calls object file");
    assert_output_contains(&build, "output:", "build runtime-calls fixture");
    assert!(
        nm_contains_symbol(&object_output, "str_len"),
        "expected runtime-calls object to reference str_len"
    );
    assert!(
        nm_contains_symbol(&object_output, "syscall_write"),
        "expected runtime-calls object to reference syscall_write"
    );
    assert!(
        nm_contains_symbol(&object_output, "syscall_read"),
        "expected runtime-calls object to reference syscall_read"
    );
}

#[test]
fn event_unsubscribe_fixture_jit_runs_and_aot_contains_event_symbols() {
    let workspace = E2eWorkspace::from_fixture("event_unsubscribe");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/event_unsubscribe");
    let object_output = workspace.join("out/event_unsubscribe.o");
    let cli = BeskidCliInvoker::new();

    let jit_run = cli.run([
        "run",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
    ]);
    assert_success(&jit_run, "run event-unsubscribe fixture through JIT");
    assert_output_contains(&jit_run, "0", "run event-unsubscribe fixture through JIT");

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
    assert_success(&build, "build event-unsubscribe fixture");
    assert_file_exists(&output_binary, "event-unsubscribe output binary");
    assert_file_exists(&object_output, "event-unsubscribe object file");
    assert!(
        nm_contains_symbol(&object_output, "event_subscribe"),
        "expected event-unsubscribe object to reference event_subscribe"
    );
    assert!(
        nm_contains_symbol(&object_output, "event_unsubscribe_first"),
        "expected event-unsubscribe object to reference event_unsubscribe_first"
    );
}

#[test]
fn smoke_fixture_build_graph_includes_corelib_dependency() {
    let workspace = E2eWorkspace::from_fixture("smoke_project");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/corelib_graph");
    let object_output = workspace.join("out/corelib_graph.o");
    let cli = BeskidCliInvoker::new();

    let jit_run = cli.run([
        "run",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
    ]);
    assert_success(&jit_run, "run smoke fixture through JIT");

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
    assert_success(&build, "build smoke fixture with corelib graph");
    assert_output_contains(
        &build,
        "corelib: project dependency detected",
        "build smoke fixture with corelib graph",
    );
    assert_file_exists(&output_binary, "corelib graph output binary");
    assert_file_exists(&object_output, "corelib graph object file");
    assert!(
        nm_contains_symbol(&object_output, "str_len"),
        "expected corelib graph object to reference str_len"
    );
    assert!(
        nm_contains_symbol(&object_output, "array_new"),
        "expected corelib graph object to reference array_new"
    );
}
