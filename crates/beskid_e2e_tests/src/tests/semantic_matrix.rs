use std::time::Duration;

use crate::harness::assertions::{assert_exit_code, assert_file_exists, assert_success};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::{nm_contains_symbol, run_binary};
use crate::harness::workspace::E2eWorkspace;

struct SemanticCase {
    fixture: &'static str,
    manifest: &'static str,
    expected: i32,
    native_exec_expected: bool,
    expected_symbols: &'static [&'static str],
}

const CASES: &[SemanticCase] = &[
    SemanticCase {
        fixture: "contracts_dispatch",
        manifest: "ContractsDispatch.bproj",
        expected: 42,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "enums_match",
        manifest: "EnumsMatch.bproj",
        expected: 7,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "method_dispatch",
        manifest: "MethodDispatch.bproj",
        expected: 42,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "closure_capture",
        manifest: "ClosureCapture.bproj",
        expected: 42,
        native_exec_expected: true,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "try_expression",
        manifest: "TryExpression.bproj",
        expected: 1,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "composition_app",
        manifest: "CompositionApp.bproj",
        expected: 0,
        native_exec_expected: false,
        expected_symbols: &[],
    },
];

#[test]
fn semantic_matrix_jit_and_aot_are_consistent() {
    let cli = BeskidCliInvoker::new();
    for case in CASES {
        run_case(&cli, case);
    }
}

fn run_case(cli: &BeskidCliInvoker, case: &SemanticCase) {
    let workspace = E2eWorkspace::from_fixture(case.fixture);
    let manifest = workspace.join(case.manifest);
    let output_binary = workspace.join(format!("out/{}", case.fixture));
    let object_output = workspace.join(format!("out/{}.o", case.fixture));

    let jit_run = cli.run(["run", "--project", manifest.to_str().expect("manifest path str"), "--target", "App"]);
    assert_exit_code(&jit_run, case.expected, &format!("run semantic case {} through JIT", case.fixture));

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
    assert_success(&build, &format!("build semantic case {}", case.fixture));
    assert_file_exists(&output_binary, &format!("semantic case {} output binary", case.fixture));
    assert_file_exists(&object_output, &format!("semantic case {} object output", case.fixture));

    for symbol in case.expected_symbols {
        assert!(
            nm_contains_symbol(&object_output, symbol),
            "expected object {} to contain symbol {}",
            object_output.display(),
            symbol
        );
    }

    if case.native_exec_expected {
        let native_run = run_binary(&output_binary, Duration::from_secs(10));
        assert_exit_code(&native_run, case.expected, &format!("execute semantic case {} binary", case.fixture));
    }
}

/// Host hooks run at run time, in the order the composition Standard gives, on both JIT and AOT.
#[test]
fn composition_hooks_run_in_order_on_every_exit() {
    let cli = BeskidCliInvoker::new();
    let workspace = E2eWorkspace::from_fixture("composition_hooks");
    let manifest = workspace.join("CompositionHooks.bproj");
    let expected = [
        "before launch",
        "startup hello hello",
        "init hello",
        "inside with",
        "dispose",
        "init hello",
        "early exit",
        "dispose",
        "shutdown",
    ];
    let check = |output: &std::process::Output, route: &str| {
        assert_exit_code(output, 3, &format!("composition hooks through {route}"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines = stdout.lines().map(str::trim_end).filter(|line| !line.is_empty()).collect::<Vec<_>>();
        assert_eq!(lines, expected, "{route}: hook order");
    };
    let jit_run = cli.run(["run", "--project", manifest.to_str().expect("manifest path str"), "--target", "App"]);
    check(&jit_run, "JIT");

    let output_binary = workspace.join("out/composition_hooks");
    let build = cli.run([
        "build",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
        "--output",
        output_binary.to_str().expect("output path str"),
    ]);
    assert_success(&build, "build composition hooks");
    let native_run = run_binary(&output_binary, Duration::from_secs(10));
    check(&native_run, "AOT");
}

/// `host AppHost() : ConsoleHost` inherits Corelib's console base host and its `CommandLine`.
#[test]
fn console_host_base_registers_the_command_line() {
    let cli = BeskidCliInvoker::new();
    let workspace = E2eWorkspace::from_fixture("console_host");
    let manifest = workspace.join("ConsoleHost.bproj");
    let run = cli.run(["run", "--project", manifest.to_str().expect("manifest path str"), "--target", "App"]);
    assert_exit_code(&run, 7, "CommandLine.Exit inside a scope ends the program through the host");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout.lines().map(str::trim_end).filter(|line| !line.is_empty()).collect::<Vec<_>>(),
        ["hello Ada 0", "enter 3", "exit", "leave 3", "shutdown"],
        "the open scope's dispose runs innermost first, then the global dispose"
    );
}
