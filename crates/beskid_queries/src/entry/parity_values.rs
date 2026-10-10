//! Query-gate parity fixtures for the `value_obligations` E1206 rows (see `parity_harness.rs`):
//! the legacy `TypeError::TypeMismatch` shapes of generic callees, contract dispatch,
//! callable-value calls, event raises, lambdas against an expected signature, contract-typed
//! destinations, generic `type` literals and `enum` constructors, nested payload patterns, and
//! `test` body returns. Generations 2000-2999 belong to this module.

use super::parity_harness::{assert_covered_kinds, assert_silent, codes, reports_kind, root_findings};

const NAMED: &str = "pub contract Named { string Name(); } pub type Plain { i64 v } ";
const HUB: &str = "pub type Hub { event OnPing(i64 value), pub unit Fire() { this.OnPing(1_i64); } } ";

/// (legacy class, variant, code, source): every row carries the exact `TypeMismatch` variant.
const COVERED_VALUE_KINDS: &[(&str, &str, &str, &str)] = &[
    (
        "generic callee, parameter without generics (row a)",
        "TypeMismatch",
        "E1206",
        "unit Take<T>(T v, bool flag) { return; } unit Main() { Take(1_i64, 7_i64); return; }",
    ),
    (
        "generic callee, explicit type argument fixes the parameter (row a)",
        "TypeMismatch",
        "E1206",
        "unit Take<T>(T v) { return; } unit Main() { Take<i64>(true); return; }",
    ),
    (
        "generic method of a generic owner, parameter without generics (row a)",
        "TypeMismatch",
        "E1206",
        "pub type Box<T> { T value, pub unit Set(T value, bool flag) { return; } } \
         unit Main(Box<i64> b) { b.Set(1_i64, 2_i64); return; }",
    ),
    (
        "contract dispatch argument (row b)",
        "TypeMismatch",
        "E1206",
        "pub contract Named { string Name(bool loud); } string Describe(Named n) { return n.Name(1_i64); }",
    ),
    (
        "callable-value call argument (row c)",
        "TypeMismatch",
        "E1206",
        "i64 Apply(i64(bool) f) { return f(1_i64); }",
    ),
    (
        "callable-value call result into a declared destination (row c)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } string Apply(Pair(bool) f) { return f(true); }",
    ),
    (
        "event raise argument (row d)",
        "TypeMismatch",
        "E1206",
        "pub type Hub { event OnPing(i64 value), pub unit Fire() { this.OnPing(true); } }",
    ),
    (
        "lambda body against a typed let (row e)",
        "TypeMismatch",
        "E1206",
        "unit Main() { i64(i64) f = (i64 x) => true; return; }",
    ),
    (
        "lambda body against a call parameter of function type (row e)",
        "TypeMismatch",
        "E1206",
        "unit Run(i64(i64) f) { return; } unit Main() { Run((i64 x) => true); return; }",
    ),
    (
        "event subscription handler signature (row e)",
        "TypeMismatch",
        "E1206",
        "pub type Hub { event OnPing(i64 value) } unit Main(Hub hub) { hub.OnPing += ((bool x) => x); return; }",
    ),
    (
        "non-conforming nominal into a contract-typed let (row f)",
        "TypeMismatch",
        "E1206",
        "pub contract Named { string Name(); } pub type Plain { i64 v } unit Main() { Named n = Plain { v: 1 }; return; }",
    ),
    (
        "non-conforming nominal into a contract-typed parameter (row f)",
        "TypeMismatch",
        "E1206",
        "pub contract Named { string Name(); } pub type Plain { i64 v } \
         unit Show(Named n) { return; } unit Main() { Show(Plain { v: 1 }); return; }",
    ),
    (
        "non-conforming nominal into a contract-typed return (row f)",
        "TypeMismatch",
        "E1206",
        "pub contract Named { string Name(); } pub type Plain { i64 v } Named Make() { return Plain { v: 1 }; }",
    ),
    (
        "primitive into a contract-typed let (row f)",
        "TypeMismatch",
        "E1206",
        "pub contract Named { string Name(); } unit Main() { Named n = 1_i64; return; }",
    ),
    (
        "generic type literal, field without generics (row g)",
        "TypeMismatch",
        "E1206",
        "pub type Box<T> { T value, bool flag } unit Main() { Box<i64> b = Box<i64> { value: 1_i64, flag: 3_i64 }; return; }",
    ),
    (
        "generic type literal, field fixed by the literal path (row g)",
        "TypeMismatch",
        "E1206",
        "pub type Box<T> { T value } unit Main() { Box<i64> b = Box<i64> { value: true }; return; }",
    ),
    (
        "generic type literal, field fixed by the typed let (row g)",
        "TypeMismatch",
        "E1206",
        "pub type Box<T> { T value } unit Main() { Box<i64> b = Box { value: true }; return; }",
    ),
    (
        "generic enum constructor, field without generics (row g)",
        "TypeMismatch",
        "E1206",
        "pub enum Opt<T> { Some(T value, bool flag), None } unit Main() { Opt<i64> o = Opt::Some(1_i64, 3_i64); return; }",
    ),
    (
        "generic enum constructor, field fixed by the typed let (row g)",
        "TypeMismatch",
        "E1206",
        "pub enum Opt<T> { Some(T value), None } unit Main() { Opt<i64> o = Opt::Some(true); return; }",
    ),
    (
        "generic enum constructor, field fixed by the return destination (row g)",
        "TypeMismatch",
        "E1206",
        "pub enum Opt<T> { Some(T value), None } Opt<i64> Make() { return Opt::Some(true); }",
    ),
    (
        "nested enum pattern against the variant field (row h)",
        "TypeMismatch",
        "E1206",
        "enum Inner { X, Y } enum Other { Z } enum Outer { Wrap(Inner inner), Empty } \
         i64 Main(Outer o) { return match o { Outer::Wrap(Other::Z) => 0_i64, _ => 1_i64, }; }",
    ),
    (
        "nested literal pattern against the variant field (row h)",
        "TypeMismatch",
        "E1206",
        "enum Outer { Wrap(bool inner), Empty } i64 Main(Outer o) { return match o { Outer::Wrap(1) => 0_i64, _ => 1_i64, }; }",
    ),
    (
        "nested literal pattern against a generic variant field fixed by the scrutinee (row h)",
        "TypeMismatch",
        "E1206",
        "pub enum Opt<T> { Some(T value), None } i64 Main(Opt<bool> o) { return match o { Opt::Some(1) => 0_i64, _ => 1_i64, }; }",
    ),
    (
        "nominal return in a test body (row i)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } test T { return Pair { a: 1 }; }",
    ),
];

#[test]
fn every_value_obligation_row_is_reported_by_the_query_gate_alone() {
    assert_covered_kinds(COVERED_VALUE_KINDS, 2000);
}

/// Well-typed shapes of every row: the query gate stays silent.
const SILENT_VALUE_SHAPES: &[(&str, &str)] = &[
    (
        "generic callee with matching arguments",
        "unit Take<T>(T v, bool flag) { return; } unit Main() { Take(1_i64, true); Take<i64>(2_i64, false); return; }",
    ),
    (
        "generic method of a generic owner with matching arguments",
        "pub type Box<T> { T value, pub unit Set(T value, bool flag) { return; } } \
         unit Main(Box<i64> b) { b.Set(1_i64, true); return; }",
    ),
    (
        "contract dispatch with matching arguments",
        "pub contract Named { string Name(bool loud); } string Describe(Named n) { return n.Name(true); }",
    ),
    (
        "callable-value calls with matching arguments and results",
        "pub type Pair { i64 a } Pair Apply(Pair(bool) f) { return f(true); } i64 Count(i64(i64) g) { return g(1_i64); }",
    ),
    ("event raise with matching arguments", HUB),
    (
        "lambdas against typed destinations and a unit-valued subscription",
        "unit Sink(i64 v) { return; } unit Run(i64(i64) f) { return; } pub type Hub { event OnPing(i64 value) } \
         unit Main(Hub hub) { i64(i64) f = (i64 x) => x; Run((i64 x) => x); Run(f); hub.OnPing += ((i64 x) => Sink(x)); return; }",
    ),
    (
        "conforming nominal into contract-typed destinations",
        "pub contract Named { string Name(); } pub type Plain : Named { i64 v, pub string Name() { return \"p\"; } } \
         pub type Other { i64 w } impl Other : Named { pub string Name() { return \"o\"; } } \
         unit Show(Named n) { return; } Named Make() { return Plain { v: 1 }; } \
         unit Main() { Named n = Plain { v: 1 }; Named m = Other { w: 1 }; Show(Other { w: 2 }); return; }",
    ),
    (
        "generic literals and constructors with matching fields",
        "pub type Box<T> { T value, bool flag } pub enum Opt<T> { Some(T value, bool flag), None } \
         unit Main() { Box<i64> b = Box<i64> { value: 1_i64, flag: true }; Box<bool> c = Box<bool> { value: true, flag: false }; \
         Opt<i64> o = Opt::Some(1_i64, true); Opt<bool> p = Opt::Some(false, true); return; }",
    ),
    (
        "nested payload patterns of matching types",
        "enum Inner { X, Y } enum Outer { Wrap(Inner inner), Lit(bool flag), Empty } pub enum Opt<T> { Some(T value), None } \
         i64 Main(Outer o) { return match o { Outer::Wrap(Inner::X) => 0_i64, Outer::Wrap(i) => 1_i64, Outer::Lit(true) => 2_i64, _ => 3_i64, }; } \
         i64 Pick(Opt<i64> o) { return match o { Opt::Some(1) => 0_i64, _ => 1_i64, }; }",
    ),
    ("test body without a valued return", "pub type Pair { i64 a } test T { Pair p = Pair { a: 1 }; i64 v = 1; return; }"),
];

#[test]
fn well_typed_value_shapes_produce_no_finding() {
    assert_silent(SILENT_VALUE_SHAPES, 2100);
}

/// A generic callee's non-generic parameter is judged once, at the mismatching argument only.
#[test]
fn generic_callee_argument_is_reported_once() {
    let findings =
        root_findings("unit Take<T>(T v, bool flag) { return; } unit Main() { Take(1_i64, 7_i64); return; }", 2200);
    assert_eq!(codes(&findings), vec!["E1206"], "{findings:?}");
}

/// A nested payload pattern is judged once, at the nested pattern, not at the outer one.
#[test]
fn nested_pattern_mismatch_is_reported_once() {
    let findings = root_findings(
        "enum Inner { X, Y } enum Other { Z } enum Outer { Wrap(Inner inner), Empty } \
         i64 Main(Outer o) { return match o { Outer::Wrap(Other::Z) => 0_i64, _ => 1_i64, }; }",
        2201,
    );
    assert_eq!(codes(&findings), vec!["E1206"], "{findings:?}");
}

/// An explicitly typed lambda parameter that differs from the expected parameter type is not a
/// finding: the legacy `type_lambda_expression_with_expected` adopts the explicit type without
/// comparing it, and only the body is judged against the expected result.
#[test]
fn explicit_lambda_parameter_type_is_not_judged_against_the_expected_parameter() {
    let findings = root_findings("unit Main() { i64(i64) f = (bool x) => 1_i64; return; }", 2202);
    assert!(!reports_kind(&findings, "TypeMismatch", "E1206", None), "{findings:?}");
}

/// A `test` body's return of `unit`-typed or `never`-typed values stays silent, while a nominal
/// value is E1206 against `unit`.
#[test]
fn test_body_return_is_judged_against_unit() {
    let findings = root_findings(&format!("{NAMED}test T {{ return Plain {{ v: 1 }}; }}"), 2203);
    assert!(reports_kind(&findings, "TypeMismatch", "E1206", None), "{findings:?}");
}
