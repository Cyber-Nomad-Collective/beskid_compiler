//! `beskid fmt` keeps every attribute the 0.6.0 grammar accepts. Functions, methods, fields and
//! contract method signatures lost their attributes before; each site round-trips here.

use beskid_analysis::format::format_program;
use beskid_analysis::services::parse_program;

fn assert_round_trip(source: &str, expected: &[&str]) {
    let program = parse_program(source).unwrap_or_else(|error| panic!("source must parse: {error:?}\n{source}"));
    let once = format_program(&program).expect("format");
    let reparsed = parse_program(&once).unwrap_or_else(|error| panic!("formatted output must parse: {error:?}\n{once}"));
    let twice = format_program(&reparsed).expect("format again");
    assert_eq!(once, twice, "format must be a fixed point:\n{once}");
    for fragment in expected {
        assert!(once.contains(fragment), "formatted output lacks {fragment:?}:\n{once}");
    }
}

#[test]
fn function_attributes_round_trip() {
    assert_round_trip(
        "[Export(Abi:\"C\", Symbol:\"answer\")]\npub i64 Answer() { return 42; }",
        &["[Export(Abi: \"C\", Symbol: \"answer\")]\npub i64 Answer()"],
    );
}

#[test]
fn method_attributes_round_trip_in_type_and_impl_bodies() {
    assert_round_trip(
        "pub attribute Traced(MethodDefinition) { }\n\
         pub type Counter { i64 count, [Traced] pub i64 Read() { return this.count; } }\n\
         impl Counter { [Traced] pub i64 Twice() { return this.count * 2; } }",
        &["[Traced]\n    pub i64 Read()", "[Traced]\n    pub i64 Twice()"],
    );
}

#[test]
fn field_attributes_round_trip() {
    assert_round_trip(
        "pub attribute Slot(Field) { }\npub type Point { [Slot] i64 x, i64 y }",
        &["[Slot]\n    i64 x,"],
    );
}

#[test]
fn contract_method_attributes_round_trip() {
    assert_round_trip(
        "[Extern(Abi:\"C\", Library:\"libc\")]\npub contract Libc { [Traced] i32 getpid(); }",
        &["[Extern(Abi: \"C\", Library: \"libc\")]\npub contract Libc", "[Traced]\n    i32 getpid();"],
    );
}

#[test]
fn item_attributes_keep_round_tripping() {
    assert_round_trip(
        "[Shape]\npub type Box { i64 width }\n[Kind]\npub enum Mode { Fast, Slow }\n[Api]\npub contract Shown { i64 Show(); }",
        &["[Shape]\npub type Box", "[Kind]\npub enum Mode", "[Api]\npub contract Shown"],
    );
}
