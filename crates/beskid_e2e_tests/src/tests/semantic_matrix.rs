use std::time::Duration;

use crate::harness::assertions::{
    assert_exit_code, assert_file_exists, assert_output_contains, assert_success,
};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::{nm_contains_symbol, run_binary};
use crate::harness::workspace::E2eWorkspace;

struct SemanticCase {
    fixture: &'static str,
    expected: i32,
    native_exec_expected: bool,
    expected_symbols: &'static [&'static str],
}

const CASES: &[SemanticCase] = &[
    SemanticCase {
        fixture: "contracts_dispatch",
        expected: 42,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "enums_match",
        expected: 7,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "method_dispatch",
        expected: 42,
        native_exec_expected: false,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "closure_capture",
        expected: 42,
        native_exec_expected: true,
        expected_symbols: &[],
    },
    SemanticCase {
        fixture: "try_expression",
        expected: 1,
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
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join(format!("out/{}", case.fixture));
    let object_output = workspace.join(format!("out/{}.o", case.fixture));

    let jit_run = cli.run([
        "run",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
    ]);
    assert_success(
        &jit_run,
        &format!("run semantic case {} through JIT", case.fixture),
    );
    assert_output_contains(
        &jit_run,
        &case.expected.to_string(),
        &format!("run semantic case {} through JIT", case.fixture),
    );

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
    assert_file_exists(
        &output_binary,
        &format!("semantic case {} output binary", case.fixture),
    );
    assert_file_exists(
        &object_output,
        &format!("semantic case {} object output", case.fixture),
    );

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
        assert_exit_code(
            &native_run,
            case.expected,
            &format!("execute semantic case {} binary", case.fixture),
        );
    }
}
