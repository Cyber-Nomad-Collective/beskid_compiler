//! Type-error diagnostic codes from the lower spine (`lower.type_check`).
//!
//! Conformance locks stable `E12xx` codes through `prepare_compilation_diagnostics`; tests
//! intentionally do **not** assert pipeline phase ordering (diagnostic order may shift).

use std::collections::HashSet;
use std::fs;

use beskid_analysis::SemanticDiagnostic;
use beskid_analysis::services::{
    FrontEndOptions, PrepareOptions, prepare_compilation_diagnostics,
    resolved_input_from_plan,
};

use crate::projects::with_cwd;
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

struct TypeDiagnosticCase {
    name: &'static str,
    source: &'static str,
    expected_codes: &'static [&'static str],
}

fn diagnostic_codes(diagnostics: &[SemanticDiagnostic]) -> HashSet<String> {
    diagnostics
        .iter()
        .filter_map(|diag| diag.code.clone())
        .collect()
}

fn prepare_diagnostic_codes(source: &str) -> HashSet<String> {
    let root = temp_case_dir("type_check_diag");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "TypeCheckDiag"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    let codes = with_cwd(&root, || {
        let plan = beskid_analysis::services::compile_plan_for_input_path(&entry)
            .expect("compile plan");
        let resolved = resolved_input_from_plan(
            entry.clone(),
            source.to_string(),
            plan,
            None,
            None,
        );
        let (_, diagnostics) = prepare_compilation_diagnostics(
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions {
                    with_semantic_diagnostics: true,
                    ..Default::default()
                },
            },
            None,
        )
        .expect("prepare diagnostics");
        diagnostic_codes(&diagnostics)
    });

    let _ = fs::remove_dir_all(root);
    codes
}

fn assert_type_diagnostic_case(case: &TypeDiagnosticCase) {
    let codes = prepare_diagnostic_codes(case.source);
    for expected in case.expected_codes {
        assert!(
            codes.contains(*expected),
            "{}: expected diagnostic code {expected}, got {codes:?}",
            case.name
        );
    }
}

const LOWER_TYPE_ERROR_CASES: &[TypeDiagnosticCase] = &[
    TypeDiagnosticCase {
        name: "type_mismatch",
        source: "unit Main() { bool x = 1; }",
        expected_codes: &["E1206"],
    },
    TypeDiagnosticCase {
        name: "invalid_member_target",
        source: "unit Main() { i64 x = 1; i64 y = x.foo; }",
        expected_codes: &["E1213"],
    },
    TypeDiagnosticCase {
        name: "invalid_identity_equality",
        source: "bool Main() { return 1 === 1; }",
        expected_codes: &["E1209"],
    },
    TypeDiagnosticCase {
        name: "call_arity_mismatch",
        source: "i64 add(i64 a, i64 b) { return a + b; } unit Main() { i64 x = add(1); }",
        expected_codes: &["E1204"],
    },
    TypeDiagnosticCase {
        name: "missing_generic_args",
        source: "unit noop<T>() { } unit Main() { noop(); }",
        expected_codes: &["E1203"],
    },
    TypeDiagnosticCase {
        name: "invalid_try_on_non_result",
        source: "i64 foo() { return 1; } i64 Main() { i64 value = foo()?; return value; }",
        expected_codes: &["E1222"],
    },
    TypeDiagnosticCase {
        name: "non_bool_condition",
        source: "unit Main() { if 1 { i64 x = 1; } }",
        expected_codes: &["E1208"],
    },
];

#[test]
fn prepare_spine_emits_lower_type_error_codes() {
    for case in LOWER_TYPE_ERROR_CASES {
        assert_type_diagnostic_case(case);
    }
}

#[test]
fn prepare_spine_emits_immutable_assignment_from_semantic_stage() {
    let codes = prepare_diagnostic_codes("unit Main() { i64 x = 1; x = 2; }");
    assert!(
        codes.contains("E1214"),
        "expected immutable-assignment E1214 from semantic structural check, got {codes:?}"
    );
}
