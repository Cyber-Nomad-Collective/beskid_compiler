//! Contextual typing of numeric literals (0.6.7): an unsuffixed literal takes the type its
//! context gives it and must fit it (T0905 at the literal otherwise, never a silent wrap); a
//! suffix fixes the type and wins over the context, and a suffix that differs from a numeric
//! context is a type mismatch (E1206). Syntax generations 9400-9599.

use super::parity_harness::{assert_covered_kinds, assert_silent, root_findings};
use beskid_analysis::analysis::SemanticIssueKind;

const NARROW_TYPES: &str = "pub enum Ip { V4(u8 a, u8 b), } pub type Port { u8 high; u8 low; } \
                            pub type Sink { i64 marker; pub unit Put(u8 b) { return; } } \
                            unit Take(u8 v) { return; } ";

fn source(body: &str) -> String {
    format!("{NARROW_TYPES}{body}")
}

/// One fixture per context that gives an unsuffixed literal its type.
fn out_of_range_in_every_context() -> Vec<(&'static str, String)> {
    vec![
        ("function argument", source("unit Main() { Take(300); return; }")),
        ("method argument", source("unit Main(Sink sink) { sink.Put(300); return; }")),
        ("enum variant constructor", source("unit Main() { Ip address = Ip::V4(300, 0); return; }")),
        ("declared local", source("unit Main() { u8 x = 300; return; }")),
        ("type literal field", source("unit Main() { Port port = Port { high: 300, low: 0 }; return; }")),
        ("return value", source("u8 F() { return 300; }")),
        ("array element", source("unit Main() { u8[] bytes = [1, 300]; return; }")),
        ("assignment target", source("unit Main(mut u8 b) { b = 300; return; }")),
        ("binary operand", source("bool F(u8 b) { return b == 300; }")),
        ("negative to an unsigned argument", source("unit Main() { Take(-1); return; }")),
        ("negative to an unsigned field", source("unit Main() { Port port = Port { high: -1, low: 0 }; return; }")),
        ("negative to an unsigned enum payload", source("unit Main() { Ip address = Ip::V4(1, -2); return; }")),
        ("f32 local", source("unit Main() { f32 x = 1.0e40; return; }")),
        ("f32 argument", source("unit Scale(f32 factor) { return; } unit Main() { Scale(1.0e40); return; }")),
    ]
}

#[test]
fn unsuffixed_literal_out_of_range_is_t0905_in_every_context() {
    let fixtures = out_of_range_in_every_context();
    let table = fixtures
        .iter()
        .map(|(class, source)| (*class, "NumericLiteralOutOfRange", "T0905", source.as_str()))
        .collect::<Vec<_>>();
    assert_covered_kinds(&table, 9400);
}

#[test]
fn out_of_range_literal_names_its_value_and_the_context_type_at_the_literal() {
    for (generation, source, value, primitive) in [
        (9430, source("unit Main() { Ip address = Ip::V4(300, 0); return; }"), "300", "u8"),
        (9431, source("unit Main(Sink sink) { sink.Put(256); return; }"), "256", "u8"),
        (9432, source("unit Main() { Take(-1); return; }"), "-1", "u8"),
    ] {
        let findings = root_findings(&source, generation);
        let finding = findings
            .iter()
            .find(|finding| {
                matches!(&finding.kind, SemanticIssueKind::NumericLiteralOutOfRange { value: v, primitive: p }
                    if v == value && p == primitive)
            })
            .unwrap_or_else(|| panic!("expected `{value}` does not fit in `{primitive}`, got {findings:?}"));
        assert_eq!(&source[finding.span.start..finding.span.end], value);
        assert_eq!(finding.kind.message(), format!("`{value}` does not fit in `{primitive}`"));
        assert_eq!(findings.len(), 1, "{findings:?}");
    }
}

#[test]
fn unsuffixed_literals_that_fit_their_context_need_no_suffix() {
    let fixtures = [
        ("function argument", source("unit Main() { Take(255); return; }")),
        ("method argument", source("unit Main(Sink sink) { sink.Put(0); return; }")),
        ("enum variant constructor", source("unit Main() { Ip address = Ip::V4(127, 1); return; }")),
        ("type literal fields", source("unit Main() { Port port = Port { high: 0, low: 80 }; return; }")),
        ("declared local and assignment", source("unit Main(mut u8 b) { u8 x = 7; b = 255; return; }")),
        ("return value", source("u8 F() { return 200; }")),
        ("array elements", source("unit Main() { u8[] bytes = [1, 2, 255]; return; }")),
        ("binary operand", source("bool F(u8 b) { return b + 1 == 255; }")),
        ("f32 contexts", source("unit Scale(f32 factor) { return; } unit Main() { f32 x = 1.5; Scale(0.25); return; }")),
        ("integer literal at a float context", source("unit Main() { f64 x = 1; return; }")),
        ("no context keeps the default type", source("unit Main() { let x = 5; let y = 3000000000; let z = 1.5; return; }")),
        (
            "the networking example without suffixes",
            source(
                "type Address { Ip ip; Port port; } i64 Bind(Address address, i64 backlog) { return backlog; } \
                 i64 Main() { Address any = Address { ip: Ip::V4(127, 1), port: Port { high: 0, low: 0 } }; return Bind(any, 8); }",
            ),
        ),
    ];
    let table = fixtures.iter().map(|(class, source)| (*class, source.as_str())).collect::<Vec<_>>();
    assert_silent(&table, 9440);
}

/// A suffix fixes the literal's type and wins over its context.
#[test]
fn suffixes_win_over_the_context() {
    let fixtures = [
        ("matching integer suffix", source("unit Main() { u8 x = 5_u8; i64 y = 7_i64; Take(1_u8); return; }")),
        ("matching float suffix", source("unit Main() { f32 x = 1.5_f32; f64 y = 2.5_f64; return; }")),
        ("suffix in a binary operand keeps numeric promotion", "bool F(i32 x) { return x + 1_i64 > 0_i64; }".to_string()),
        ("suffixed literal without a context", source("unit Main() { let x = 300_i64; let y = 5_u8; return; }")),
    ];
    let table = fixtures.iter().map(|(class, source)| (*class, source.as_str())).collect::<Vec<_>>();
    assert_silent(&table, 9460);
}

/// A suffix that differs from a numeric context is the type-mismatch error, never a conversion;
/// so is a float literal written where an integer is required.
#[test]
fn a_suffix_conflicting_with_the_context_is_a_type_mismatch() {
    let fixtures = [
        ("integer suffix at a declared local", source("unit Main() { u8 x = 5_i64; return; }")),
        ("integer suffix at an argument", source("unit Main() { Take(5_i32); return; }")),
        ("integer suffix at a field", source("unit Main() { Port port = Port { high: 1_u16, low: 0 }; return; }")),
        ("integer suffix at an enum payload", source("unit Main() { Ip address = Ip::V4(1_i64, 0); return; }")),
        ("float suffix at another float", source("unit Main() { f64 x = 1.5_f32; return; }")),
        ("float literal at an integer local", source("unit Main() { i64 x = 1.5; return; }")),
    ];
    let table = fixtures
        .iter()
        .map(|(class, source)| (*class, "TypeMismatch", "E1206", source.as_str()))
        .collect::<Vec<_>>();
    assert_covered_kinds(&table, 9480);

    let source = source("unit Main() { u8 x = 5_i64; return; }");
    let findings = root_findings(&source, 9500);
    assert_eq!(findings.len(), 1, "one mismatch and no range error: {findings:?}");
    let finding = &findings[0];
    assert!(
        matches!(&finding.kind, SemanticIssueKind::TypeMismatch { expected_name, actual_name } if expected_name == "u8" && actual_name == "i64"),
        "{findings:?}"
    );
    assert_eq!(&source[finding.span.start..finding.span.end], "5_i64");
}

const SAME: &str = "bool Same<T>(T left, T right) { return true; } ";

/// A bare literal never decides a generic binding against an exactly typed argument: `T` is the
/// exact type, in either argument order, and a literal that does not fit it is T0905 at the
/// literal instead of a binding conflict or a truncation.
#[test]
fn a_bare_literal_that_does_not_fit_an_inferred_generic_is_t0905_at_the_literal() {
    for (generation, body, value, primitive) in [
        (9520, "bool Main() { return Same(1_u8, 300); }", "300", "u8"),
        (9521, "bool Main() { return Same(300, 1_u8); }", "300", "u8"),
        (9522, "bool Main(u8 b) { return Same(b, -1); }", "-1", "u8"),
    ] {
        let source = format!("{SAME}{body}");
        let findings = root_findings(&source, generation);
        let finding = findings
            .iter()
            .find(|finding| {
                matches!(&finding.kind, SemanticIssueKind::NumericLiteralOutOfRange { value: v, primitive: p }
                    if v == value && p == primitive)
            })
            .unwrap_or_else(|| panic!("expected `{value}` does not fit in `{primitive}` for {body}, got {findings:?}"));
        assert_eq!(&source[finding.span.start..finding.span.end], value);
        assert_eq!(findings.len(), 1, "{body}: {findings:?}");
    }
}

#[test]
fn bare_literals_take_an_inferred_generic_from_an_exact_argument_in_either_order() {
    let fixtures = [
        ("literal after the exact argument", format!("{SAME}bool Main() {{ return Same(1_u8, 200); }}")),
        ("literal before the exact argument", format!("{SAME}bool Main() {{ return Same(200, 1_u8); }}")),
        ("wide literal before an i64 argument", format!("{SAME}bool Main() {{ return Same(3000000000, 5_i64); }}")),
        ("two bare literals keep the default", format!("{SAME}bool Main() {{ return Same(1, 2); }}")),
    ];
    let table = fixtures.iter().map(|(class, source)| (*class, source.as_str())).collect::<Vec<_>>();
    assert_silent(&table, 9530);
}

/// A bare literal branch of `c ? a : b` takes the conditional's context, or without one the other
/// branch's type; two bare literals keep the default, and a typed branch never widens.
#[test]
fn conditional_literal_branches_take_the_context_or_the_other_branch() {
    let fixtures = [
        ("both literals at an i64 return", "i64 F(bool c) { return c ? 1 : 2; }".to_string()),
        ("wide literal branch at an i64 return", "i64 F(bool c) { return c ? 1 : 3000000000; }".to_string()),
        ("literal branch takes the typed branch", "i64 F(bool c, i64 x) { let y = c ? 1 : x; return y; }".to_string()),
        ("two literals without a context", "unit F(bool c) { let y = c ? 1 : 2; return; }".to_string()),
    ];
    let table = fixtures.iter().map(|(class, source)| (*class, source.as_str())).collect::<Vec<_>>();
    assert_silent(&table, 9540);
}

#[test]
fn a_conditional_literal_branch_that_does_not_fit_is_t0905_at_the_literal() {
    for (generation, source, value, primitive) in [
        (9550, "u8 F(bool c) { return c ? 1 : 300; }", "300", "u8"),
        (9551, "bool F(bool c, u8 b) { let y = c ? b : 300; return true; }", "300", "u8"),
    ] {
        let findings = root_findings(source, generation);
        let finding = findings
            .iter()
            .find(|finding| {
                matches!(&finding.kind, SemanticIssueKind::NumericLiteralOutOfRange { value: v, primitive: p }
                    if v == value && p == primitive)
            })
            .unwrap_or_else(|| panic!("expected `{value}` does not fit in `{primitive}` for {source}, got {findings:?}"));
        assert_eq!(&source[finding.span.start..finding.span.end], value);
        assert_eq!(findings.len(), 1, "{source}: {findings:?}");
    }
}

#[test]
fn a_conditional_branch_suffix_conflicting_with_the_context_is_one_type_mismatch() {
    let source = "u8 F(bool c) { return c ? 1 : 5_i64; }";
    let findings = root_findings(source, 9560);
    assert_eq!(findings.len(), 1, "one mismatch at the literal: {findings:?}");
    let finding = &findings[0];
    assert!(
        matches!(&finding.kind, SemanticIssueKind::TypeMismatch { expected_name, actual_name } if expected_name == "u8" && actual_name == "i64"),
        "{findings:?}"
    );
    assert_eq!(&source[finding.span.start..finding.span.end], "5_i64");
}
