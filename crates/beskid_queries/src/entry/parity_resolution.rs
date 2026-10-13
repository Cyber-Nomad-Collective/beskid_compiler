//! Query-gate parity fixtures (see `parity_harness.rs`) for the name-resolution, unit-level, and
//! member-path classes of `docs/reports/2026-10-05-v06-query-check-authority.md`: value paths
//! that name no value, field paths of three or more segments, E1102 across units, `mod`
//! declarations and builtin collisions, qualified type paths, generic conformances, and private
//! fields of another unit. Syntax generations 3000-3999.

use super::parity_harness::{assert_covered_kinds, assert_silent, codes, reports_kind, unit_findings};

/// Single-unit classes: (legacy class, variant, code, source).
const COVERED_KINDS: &[(&str, &str, &str, &str)] = &[
    // A named non-generic function read as a value is a closure record (`function_item_value`),
    // so `let f = Helper;` is legal. A generic function has no single entry to capture and stays an
    // item named in value position.
    (
        "TypeError::UnknownValueType (generic function named as a value)",
        "TypeUnknownValueType",
        "E1201",
        "T Helper<T>(T value) { return value; } unit Main() { let f = Helper; return; }",
    ),
    (
        "TypeError::UnknownValueType (type-qualified value path)",
        "TypeUnknownValueType",
        "E1201",
        "pub type Pair { i64 a } i64 Main() { return Pair.a; }",
    ),
    (
        "TypeError::InvalidMemberTarget (three-segment field path through a primitive)",
        "TypeInvalidMemberTarget",
        "E1213",
        "pub type Inner { i64 n } pub type Outer { Inner inner } i64 Main(Outer o) { return o.inner.n.value; }",
    ),
    (
        "TypeError::UnknownStructField (three-segment field path)",
        "TypeUnknownStructField",
        "E1211",
        "pub type Inner { i64 n } pub type Outer { Inner inner } i64 Main(Outer o) { return o.inner.missing; }",
    ),
    (
        "TypeError::UnknownStructType (field path through an enum)",
        "TypeUnknownStructType",
        "E1201",
        "enum Shape { Dot } pub type Outer { Shape s } i64 Main(Outer o) { return o.s.x; }",
    ),
    (
        "ResolveError::DuplicateItem (two mod declarations of one path)",
        "ResolveDuplicateItem",
        "E1102",
        "pub mod Hub.Child; pub mod Hub.Child; pub i64 Version() { return 6_i64; }",
    ),
    (
        "ResolveError::DuplicateItem (builtin collision)",
        "ResolveDuplicateItem",
        "E1102",
        "unit range() { return; } unit Main() { return; }",
    ),
    (
        "TypeError::ContractMethodMissingImplementation (generic implementor)",
        "ContractMethodMissingImplementation",
        "E1601",
        "pub contract Named { string Name(); } pub type Box<T> : Named { T v }",
    ),
    (
        "TypeError::ContractMethodMissingImplementation (applied generic contract)",
        "ContractMethodMissingImplementation",
        "E1601",
        "pub contract Seq<T> { i64 Count(); } pub type Numbers : Seq<i64> { i64 v }",
    ),
    (
        "TypeError::ContractAssociatedTypeMissingBinding (generic implementor)",
        "ContractAssociatedTypeMissingBinding",
        "E1607",
        "pub contract Seq { type Item; } pub type Box<T> : Seq { T v }",
    ),
    (
        "TypeError::ContractImplementationSignatureMismatch (closed signature of an applied generic contract)",
        "ContractImplementationSignatureMismatch",
        "E1602",
        "pub contract Seq<T> { i64 Count(); } pub type Numbers : Seq<i64> { i64 v, pub bool Count() { return true; } }",
    ),
];

#[test]
fn single_unit_resolution_and_member_path_classes_are_reported() {
    assert_covered_kinds(COVERED_KINDS, 3000);
}

/// Well-formed neighbours of every single-unit class above stay silent.
const SILENT: &[(&str, &str)] = &[
    ("constant read", "const N = 1; i64 Main() { return N; }"),
    ("function call", "i64 Helper() { return 1_i64; } i64 Main() { return Helper(); }"),
    ("local read", "i64 Main(i64 n) { i64 m = n; return m; }"),
    (
        "three-segment field path",
        "pub type Inner { i64 n } pub type Outer { Inner inner } i64 Main(Outer o) { return o.inner.n; }",
    ),
    ("one mod declaration", "pub mod Hub.Child; pub i64 Version() { return 6_i64; }"),
    ("builtin-like name of another spelling", "unit Range() { return; } unit Main() { return; }"),
    (
        "generic implementor with the method",
        "pub contract Named { string Name(); } pub type Box<T> : Named { T v, pub string Name() { return \"box\"; } }",
    ),
    (
        "applied generic contract with an open signature",
        "pub contract Seq<T> { T First(); } pub type Numbers : Seq<i64> { i64 v, pub i64 First() { return v; } }",
    ),
    (
        "generic implementor with the associated binding",
        "pub contract Seq { type Item; } pub type Box<T> : Seq { T v type Item = T; }",
    ),
];

#[test]
fn well_formed_resolution_and_member_paths_are_silent() {
    assert_silent(SILENT, 3100);
}

fn assert_reports(units: &[(&str, &str)], generation: u64, variant: &str, code: &str, unit: usize) {
    let findings = unit_findings(units, generation);
    assert!(
        reports_kind(&findings, variant, code, Some(unit)),
        "expected {variant} ({code}) in unit {unit}, got {:?}: {findings:?}",
        codes(&findings)
    );
}

fn assert_units_silent(units: &[(&str, &str)], generation: u64) {
    let findings = unit_findings(units, generation);
    assert!(findings.is_empty(), "expected no finding, got {findings:?}");
}

/// Two non-entry units of one logical module (`Lib.bd` and `Lib/Lib.bd` are both module `Lib`)
/// declaring one name: the legacy resolver collects them into one module scope in assembly order,
/// so the later unit carries the finding.
#[test]
fn duplicate_item_across_units_of_one_module_is_reported_in_the_later_unit() {
    let main = ("Main.bd", "unit Main() { return; }");
    assert_reports(
        &[main, ("Lib.bd", "pub i64 Helper() { return 1_i64; }"), ("Lib/Lib.bd", "pub i64 Helper() { return 2_i64; }")],
        3200,
        "ResolveDuplicateItem",
        "E1102",
        2,
    );
    assert_units_silent(
        &[main, ("Lib.bd", "pub i64 Helper() { return 1_i64; }"), ("Lib/Lib.bd", "pub i64 Other() { return 2_i64; }")],
        3201,
    );
}

const HELPER_TYPES: (&str, &str) = ("Lib/Helper.bd", "type Hidden { i64 n } pub type Shown { i64 n }");

/// Qualified type paths keep the legacy resolver's classes: private (E1107), missing in the
/// module (E1201 `ResolveUnknownTypeInModule`), and unknown module (E1108).
#[test]
fn qualified_type_paths_are_classified_like_the_legacy_resolver() {
    let cases = [
        ("use Lib.Helper; unit Main(Helper.Hidden h) { return; }", "ResolvePrivateItemInModule", "E1107"),
        ("use Lib.Helper; unit Main(Helper.Missing h) { return; }", "ResolveUnknownTypeInModule", "E1201"),
        ("unit Main(Nowhere.Thing h) { return; }", "ResolveUnknownModulePath", "E1108"),
    ];
    for (offset, (main, variant, code)) in cases.into_iter().enumerate() {
        assert_reports(&[("Main.bd", main), HELPER_TYPES], 3300 + offset as u64, variant, code, 0);
    }
    assert_units_silent(&[("Main.bd", "use Lib.Helper; unit Main(Helper.Shown h) { return; }"), HELPER_TYPES], 3310);
}

/// A hub that re-exports a child through `pub mod` serves the child's members under qualified
/// paths; a name two children provide at the same distance is ambiguous (E1104) in type and value
/// position, and the hub's own declaration shadows a re-exported one. A qualified callee through
/// a live import binding is not classified here: `path_call_resolution` lowers it as `Dynamic`
/// (`imported_call_receiver_exists`) before `unresolved_call_target` can judge it.
#[test]
fn qualified_hub_paths_follow_the_reexport_rule() {
    let hub = ("Lib/Hub.bd", "pub mod Lib.Hub.Left; pub mod Lib.Hub.Right; pub i64 Own() { return 0_i64; }");
    let left = (
        "Lib/Hub/Left.bd",
        "pub i64 Shared() { return 1_i64; } pub i64 Only() { return 1_i64; } pub type Twin { i64 n } pub type Solo { i64 n } pub i64 Own() { return 1_i64; }",
    );
    let right = ("Lib/Hub/Right.bd", "pub i64 Shared() { return 2_i64; } pub type Twin { i64 n }");
    assert_units_silent(
        &[("Main.bd", "use Lib.Hub; i64 Main(Hub.Solo s) { return Hub.Only() + Hub.Own(); }"), hub, left, right],
        3500,
    );
    let cases = [
        ("use Lib.Hub; unit Main(Hub.Twin t) { return; }", "ResolveAmbiguousItemInModule", "E1104"),
        ("use Lib.Hub; unit Main() { let f = Hub.Shared; return; }", "ResolveAmbiguousItemInModule", "E1104"),
        ("use Lib.Hub; unit Main() { let f = Hub.Only; return; }", "TypeUnknownValueType", "E1201"),
    ];
    for (offset, (main, variant, code)) in cases.into_iter().enumerate() {
        assert_reports(&[("Main.bd", main), hub, left, right], 3510 + offset as u64, variant, code, 0);
    }
}

const HELPER_VALUES: (&str, &str) = ("Lib/Helper.bd", "pub i64 Run() { return 1_i64; } i64 Hidden() { return 2_i64; }");

/// Module-qualified value paths that are not call targets: unknown module (E1108), unknown value
/// in the module (E1101), private item (E1107), and a resolved item used as a value (E1201).
#[test]
fn qualified_value_paths_are_classified_like_qualified_callees() {
    let cases = [
        ("use Lib.Helper; unit Main() { let f = Helper.Missing; return; }", "ResolveUnknownValueInModule", "E1101"),
        ("use Lib.Helper; unit Main() { let f = Helper.Hidden; return; }", "ResolvePrivateItemInModule", "E1107"),
        ("unit Main() { let f = Nowhere.Thing; return; }", "ResolveUnknownModulePath", "E1108"),
        ("use Lib.Helper; unit Main() { let f = Helper.Run; return; }", "TypeUnknownValueType", "E1201"),
    ];
    for (offset, (main, variant, code)) in cases.into_iter().enumerate() {
        assert_reports(&[("Main.bd", main), HELPER_VALUES], 3400 + offset as u64, variant, code, 0);
    }
    assert_units_silent(&[("Main.bd", "use Lib.Helper; i64 Main() { return Helper.Run(); }"), HELPER_VALUES], 3410);
}

/// A field without `pub` declared in another unit is inaccessible through a projection.
#[test]
fn private_field_of_another_unit_is_inaccessible() {
    let main = ("Main.bd", "use Lib.Secret; i64 Main(Secret.Secret s) { return s.hidden; }");
    assert_reports(
        &[main, ("Lib/Secret.bd", "pub type Secret { i64 hidden }")],
        3500,
        "TypeInaccessibleStructField",
        "E1211",
        0,
    );
    assert_units_silent(&[main, ("Lib/Secret.bd", "pub type Secret { pub i64 hidden }")], 3501);
}
