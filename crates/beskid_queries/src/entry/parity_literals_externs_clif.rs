//! Query-gate parity fixtures for the unsuffixed numeric-literal (T0905), optional extern
//! (T0906), clif block (E1232), test-body `return` (E1206), and fiber join deadlock (E1224)
//! classes (see `parity_harness.rs`). Syntax generations 1000-1999.

use super::parity_harness::{assert_covered_kinds, assert_silent, codes, reports_kind, root_findings};
use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::services::SemanticFactFinding;

/// Unsuffixed integer literals out of the range of their declared destination: the legacy
/// `type_id_for_literal` / `type_unary_expression` under `contextual_expected_type`.
const UNSUFFIXED_OUT_OF_RANGE: &[(&str, &str, &str, &str)] = &[
    (
        "NumericLiteralOutOfRange (typed let)",
        "NumericLiteralOutOfRange",
        "T0905",
        "unit Main() { i8 x = 300; return; }",
    ),
    ("NumericLiteralOutOfRange (return value)", "NumericLiteralOutOfRange", "T0905", "i8 F() { return 300; }"),
    (
        "NumericLiteralOutOfRange (direct call argument)",
        "NumericLiteralOutOfRange",
        "T0905",
        "unit Take(i8 v) { return; } unit Main() { Take(300); return; }",
    ),
    (
        "NumericLiteralOutOfRange (negated literal)",
        "NumericLiteralOutOfRange",
        "T0905",
        "unit Main() { i8 x = -200; return; }",
    ),
    (
        "NumericLiteralOutOfRange (grouped literal)",
        "NumericLiteralOutOfRange",
        "T0905",
        "unit Main() { u8 x = (256); return; }",
    ),
    (
        "NumericLiteralOutOfRange (unsigned destination, negated literal)",
        "NumericLiteralOutOfRange",
        "T0905",
        "word F() { return -1; }",
    ),
];

#[test]
fn unsuffixed_literal_out_of_its_destination_range_is_reported() {
    assert_covered_kinds(UNSUFFIXED_OUT_OF_RANGE, 1000);
}

#[test]
fn unsuffixed_literal_out_of_range_names_the_destination_primitive_at_the_value() {
    let source = "unit Main() { i8 x = -200; return; }";
    let findings = root_findings(source, 1010);
    let finding = findings
        .iter()
        .find(|finding| matches!(&finding.kind, SemanticIssueKind::NumericLiteralOutOfRange { primitive } if primitive == "i8"))
        .unwrap_or_else(|| panic!("expected an i8 T0905, got {findings:?}"));
    assert_eq!(&source[finding.span.start..finding.span.end], "-200");
    assert_eq!(findings.len(), 1, "{findings:?}");
}

#[test]
fn in_range_and_destinationless_unsuffixed_literals_are_silent() {
    assert_silent(
        &[
            ("in-range typed let", "unit Main() { i8 x = 127; i8 y = -128; u8 z = 255; return; }"),
            ("in-range return", "i8 F() { return 100; }"),
            ("in-range call argument", "unit Take(i8 v) { return; } unit Main() { Take(-5); return; }"),
            ("no destination", "unit Main() { let x = 300; return; }"),
            ("wide destination", "unit Main() { i64 x = 3000000000; u64 y = 18446744073709551615; return; }"),
        ],
        1020,
    );
}

/// The `Optional` argument of an `[Extern]` contract and its availability query: the legacy
/// `validate_extern_contract` and `validate_extern_availability_method`, now T0906.
#[test]
fn invalid_optional_extern_contracts_are_reported_as_t0906() {
    assert_covered_kinds(
        &[
            (
                "ExternInvalidOptional (non-boolean Optional)",
                "ExternInvalidOptional",
                "T0906",
                "[Extern(Abi:\"C\", Library:\"m\", Optional:\"yes\")] contract Native { i64 Abs(i64 v); }",
            ),
            (
                "ExternInvalidOptional (malformed Available)",
                "ExternInvalidOptional",
                "T0906",
                "[Extern(Abi:\"C\", Library:\"m\", Optional:true)] contract Native { i64 Available(i64 v); i64 Abs(i64 v); }",
            ),
        ],
        1100,
    );
}

#[test]
fn non_boolean_optional_is_reported_at_the_argument_before_the_abi() {
    let source = "[Extern(Abi:\"Rust\", Optional:1)] contract Native { i64 Abs(i64 v); }";
    let findings = root_findings(source, 1110);
    assert_eq!(codes(&findings), vec!["T0906"], "{findings:?}");
    assert_eq!(&source[findings[0].span.start..findings[0].span.end], "Optional:1");
    assert_eq!(
        findings[0].kind.help().as_deref(),
        Some("Optional takes a boolean literal: Optional:true or Optional:false")
    );
}

#[test]
fn malformed_availability_query_is_reported_at_its_signature_and_exempt_from_the_c_profile() {
    let source = "[Extern(Abi:\"C\", Library:\"m\", Optional:true)] contract Native { string Available(); i64 Abs(i64 v); }";
    let findings = root_findings(source, 1111);
    assert_eq!(codes(&findings), vec!["T0906"], "{findings:?}");
    assert!(source[findings[0].span.start..findings[0].span.end].starts_with("string Available()"), "{findings:?}");
    assert_eq!(
        findings[0].kind.help().as_deref(),
        Some("the availability query of an optional contract must be declared as `bool Available();`")
    );
}

#[test]
fn well_formed_optional_extern_contracts_are_silent() {
    assert_silent(
        &[
            (
                "Optional:true with bool Available()",
                "[Extern(Abi:\"C\", Library:\"m\", Optional:true)] contract Native { bool Available(); i64 Abs(i64 v); }",
            ),
            (
                "Optional:false",
                "[Extern(Abi:\"C\", Library:\"m\", Optional:false)] contract Native { i64 Abs(i64 v); }",
            ),
        ],
        1120,
    );
}

/// The `InvalidClifBlock` details of `findings`, in finding order.
fn clif_details(findings: &[SemanticFactFinding]) -> Vec<String> {
    findings
        .iter()
        .filter_map(|finding| match &finding.kind {
            SemanticIssueKind::InvalidClifBlock { detail } => Some(detail.clone()),
            _ => None,
        })
        .collect()
}

/// One fixture per legacy `InvalidClifBlock` detail class: (class, source, detail fragment).
const CLIF_CLASSES: &[(&str, &str, &str)] = &[
    (
        "no typed context",
        "pub i64 Main(i64 v) { let x = clif { return %0 }; return 0; }",
        "a clif block needs a typed context: use it as a return value, a typed `let` initializer, or a call argument",
    ),
    (
        "non-scalar context",
        "pub i8 Main() { return clif {\n%k = iconst.i64 2\nreturn %k\n}; }",
        "a clif block must produce a scalar value (bool, char, integer, f64, word, or pointer)",
    ),
    (
        "surface rejection",
        "pub i64 F(i64 value) { return clif {\n%v = jump block1\nreturn %v\n}; }",
        "opcode `jump` is not allowed",
    ),
    (
        "parameter out of range",
        "pub i64 F(i64 value) { return clif { return %3 }; }",
        "`%3` does not name a parameter; the enclosing function has 1 CLIF parameter(s)",
    ),
    (
        "array used as a value",
        "pub i64 F(i64[] values) { return clif {\n%v = iadd %0, %0\nreturn %v\n}; }",
        "`%0` is not a scalar parameter; read arrays through `payload %0` and `length %0`",
    ),
    (
        "non-clif-scalar primitive used as a value",
        "pub i64 F(i8 value) { return clif { return %0 }; }",
        "`%0` is not a scalar parameter; read arrays through `payload %0` and `length %0`",
    ),
    (
        "payload of a non-payload parameter",
        "pub i64 F(string text) { return clif {\n%p = payload %0\n%v = load.i64 %p\nreturn %v\n}; }",
        "`payload`/`length` require a parameter of type u8[], u32[], i64[]",
    ),
    (
        "method receiver",
        "pub type Box { i64 value, pub i64 Get() { return clif { return %0 }; } }",
        "`%0` is a method receiver or has an unresolved type",
    ),
    (
        "parameters inside a lambda",
        "pub i64 Outer(i64 value) { (i64) => i64 f = (i64 x) => { i64 k = clif { return %0 }; return k; }; return f(value); }",
        "clif blocks inside lambdas cannot name parameters with `%N`",
    ),
];

#[test]
fn every_invalid_clif_block_class_is_reported_with_the_legacy_detail() {
    let mut failures = Vec::new();
    for (offset, (class, source, detail)) in CLIF_CLASSES.iter().enumerate() {
        let findings = root_findings(source, 1200 + offset as u64);
        let details = clif_details(&findings);
        if !reports_kind(&findings, "InvalidClifBlock", "E1232", None) || !details.iter().any(|found| found.contains(detail)) {
            failures.push(format!("{class}: expected E1232 `{detail}`, got {:?} {details:?}\n  {source}", codes(&findings)));
        }
    }
    assert!(failures.is_empty(), "query gate misses {} clif class(es):\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn invalid_clif_block_is_reported_at_the_block() {
    let source = "pub i64 F(i64 value) { return clif { return %3 }; }";
    let findings = root_findings(source, 1220);
    assert_eq!(codes(&findings), vec!["E1232"], "{findings:?}");
    assert_eq!(&source[findings[0].span.start..findings[0].span.end], "clif { return %3 }");
}

#[test]
fn well_formed_clif_blocks_are_silent() {
    assert_silent(
        &[
            ("return value", "pub i64 Echo(i64 value) { return clif { return %0 }; }"),
            ("typed let", "pub i64 Twice(i64 value) { i64 doubled = clif {\n%d = iadd %0, %0\nreturn %d\n}; return doubled; }"),
            ("call", "pub f64 Root(f64 value) { return clif { call @sqrt(%0) }; }"),
            (
                "payload read",
                "pub u32 Mask(u32[] words, u32 mask) { return clif {\n%p = payload %0\n%w = load.i32 %p\n%m = band %w, %1\nreturn %m\n}; }",
            ),
            (
                "direct call argument",
                "pub i64 Echo(i64 value) { return value; } pub i64 Use(i64 value) { return Echo(clif { return %0 }); }",
            ),
            (
                "statement",
                "pub unit Fill(u8[] bytes, i64 value) { clif {\n%p = payload %0\n%v = ireduce.i8 %1\nistore8 %v, %p\n}; return; }",
            ),
            (
                "local assignment",
                "pub unit Bump() { mut i64 last = 0; last = clif {\n%k = iconst.i64 2\nreturn %k\n}; return; }",
            ),
            ("unit parameter skipped", "pub i64 Skip(unit u, i64 value) { return clif { return %0 }; }"),
        ],
        1230,
    );
}

/// A test body is typed against `unit` (legacy `current_return_type` of a test).
#[test]
fn valued_return_in_a_test_is_a_type_mismatch() {
    let findings = root_findings("test t { return 1_i64; }", 1300);
    assert!(
        findings.iter().any(|finding| matches!(
            &finding.kind,
            SemanticIssueKind::TypeMismatch { expected_name, actual_name } if expected_name == "unit" && actual_name == "i64"
        ) && finding.kind.code() == "E1206"),
        "{findings:?}"
    );
    assert_silent(&[("bare return in a test", "test t { return; }")], 1301);
}

/// The legacy `check_fiber_join_call` judges a join only through a handle local registered
/// after its `spawn` initializer is typed, so a join of a handle inside its own spawned lambda
/// (the only shape where the joining fiber descends from the handle's fiber) reaches the legacy
/// checker as an unresolved value, never as E1224; and `__fiber_join_status` is a private runtime
/// builtin outside the Corelib (E1107). The query gate reports E1101/E1107 there, never E1224.
#[test]
fn join_of_a_handle_inside_its_own_spawned_lambda_is_not_a_deadlock_finding() {
    let mut failures = Vec::new();
    for (offset, source) in [
        "unit Main() { let h = spawn (() => { let inner = spawn (() => __fiber_join_status(h)); return; }); return; }",
        "unit Main() { let h = spawn (() => __fiber_join_status(h)); return; }",
    ]
    .iter()
    .enumerate()
    {
        let findings = root_findings(source, 1400 + offset as u64);
        if reports_kind(&findings, "JoinWouldDeadlock", "E1224", None) {
            failures.push(format!("{source}: {:?} {findings:?}", codes(&findings)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
