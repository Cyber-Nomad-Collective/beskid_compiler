use crate::harness::assertions::{assert_output_contains, assert_success};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn analyze_reports_no_diagnostics_for_minimal_file() {
    let workspace = E2eWorkspace::from_fixture("cross_platform_cli");
    let source = workspace.join("Src/Minimal.bd");
    let cli = BeskidCliInvoker::new();

    let output = cli.run(["analyze", source.to_str().expect("source path str")]);
    assert_success(&output, "analyze minimal file");
    assert_output_contains(&output, "No diagnostics.", "analyze minimal file");
}

#[test]
fn analyze_prints_diagnostics_to_stderr_for_semantic_errors() {
    let workspace = E2eWorkspace::from_fixture("analyze_diagnostics");
    let source = workspace.join("Src/Bad.bd");
    let cli = BeskidCliInvoker::new();

    let output = cli.run(["analyze", source.to_str().expect("source path str")]);
    assert_success(&output, "analyze with diagnostics still exits 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().is_empty(),
        "expected semantic diagnostics on stderr, got empty. stdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn tree_succeeds_on_valid_source() {
    let workspace = E2eWorkspace::from_fixture("cross_platform_cli");
    let source = workspace.join("Src/Minimal.bd");
    let cli = BeskidCliInvoker::new();

    let output = cli.run(["tree", source.to_str().expect("source path str")]);
    assert_success(&output, "tree smoke file");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Program") || stdout.contains("main"),
        "tree output should mention program structure; got:\n{stdout}"
    );
}

#[test]
fn parse_succeeds_on_valid_source() {
    let workspace = E2eWorkspace::from_fixture("cross_platform_cli");
    let source = workspace.join("Src/Minimal.bd");
    let cli = BeskidCliInvoker::new();

    let output = cli.run(["parse", source.to_str().expect("source path str")]);
    assert_success(&output, "parse smoke file");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("main") || stdout.contains("Function"),
        "parse debug output should mention main or Function; got:\n{stdout}"
    );
}
