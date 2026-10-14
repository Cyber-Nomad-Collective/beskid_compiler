//! Regressions for toolchain defects found while writing the resolver in Beskid: each fixture runs
//! through `beskid run` and as a built native executable.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use crate::harness::assertions::{assert_exit_code, assert_file_exists, assert_success};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::{run_binary, run_binary_with_args};
use crate::harness::workspace::E2eWorkspace;

const EXPECTED: i32 = 42;

/// Builds `fixture` and returns its workspace and executable.
fn run_and_build(cli: &BeskidCliInvoker, fixture: &str, manifest: &str) -> (E2eWorkspace, PathBuf) {
    let workspace = E2eWorkspace::from_fixture(fixture);
    let manifest = workspace.join(manifest);
    let manifest = manifest.to_str().expect("manifest path str");
    let binary = workspace.join(format!("out/{fixture}"));
    let build = cli.run(["build", "--project", manifest, "--target", "App", "--output", binary.to_str().expect("output path str")]);
    assert_success(&build, &format!("build {fixture}"));
    assert_file_exists(&binary, &format!("{fixture} executable"));
    (workspace, binary)
}

/// Runs `fixture` through `beskid run` and as a built executable; both exit with [`EXPECTED`].
fn assert_runs(fixture: &str, manifest: &str) {
    let cli = BeskidCliInvoker::new();
    let workspace = E2eWorkspace::from_fixture(fixture);
    let project = workspace.join(manifest);
    let run = cli.run(["run", "--project", project.to_str().expect("manifest path str"), "--target", "App"]);
    assert_exit_code(&run, EXPECTED, &format!("beskid run {fixture}"));
    let (_workspace, binary) = run_and_build(&cli, fixture, manifest);
    let native = run_binary(&binary, Duration::from_secs(20));
    assert_exit_code(&native, EXPECTED, &format!("execute {fixture}"));
}

/// `cursor.text[i]`, `this.text[i]`, and a field of a field index the string directly (E2102
/// before the fix).
#[test]
fn string_fields_are_indexed_directly() {
    assert_runs("string_field_index", "StringFieldIndex.bproj");
}

/// `a.values[F(a)] = v` with `F` growing `a.values` stores into the grown array, not the
/// replaced one (a bounds trap before the fix).
#[test]
fn indexed_places_read_their_array_after_the_index_and_value() {
    assert_runs("index_evaluation_order", "IndexEvaluationOrder.bproj");
}

/// `i64[][]` and `string[][]` parse and run as arrays of arrays.
#[test]
fn nested_array_types_compile_and_run() {
    assert_runs("nested_array_types", "NestedArrayTypes.bproj");
}

/// Strings from `Core.Args` are written to stdout and used as a file path and as file contents.
#[test]
fn argument_strings_are_written_and_used_as_paths() {
    let cli = BeskidCliInvoker::new();
    let (workspace, binary) = run_and_build(&cli, "args_strings", "ArgsStrings.bproj");
    let target = workspace.join("out/written.txt");
    let output = run_binary_with_args(
        &binary,
        &[target.as_os_str(), OsStr::new("hello-args")],
        Duration::from_secs(20),
    );
    assert_exit_code(&output, EXPECTED, "execute args_strings");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "hello-args\nhello-args\nhello-args|10\nell\n",
        "argument written through Output.Write, Output.WriteLine, interpolation, and SubSlice"
    );
    assert_eq!(std::fs::read_to_string(&target).expect("file written at the argument path"), "hello-args");
}
