//! Focused reproducer for the HttpExchangeTests blocker: a `while true { match ... }` loop whose
//! body never falls through (every match arm either `return`s or `continue`s) leaves the loop's
//! `exit` block reachable only through the header's statically-dead false edge. When that loop is
//! a function's final statement, nothing terminates `exit`, and CLIF verification fails with
//! "generated statement body did not terminate its final block" (the real corelib symptom, from
//! `Http/Wire.bd`'s `ReadMessage`, which has exactly this shape and also calls `Array.Append`,
//! unrelated to the append machinery itself).

use super::support::{emit_isle_item, item_fixture};

#[test]
fn while_true_whose_body_always_returns_or_continues_terminates_its_final_block() {
    let (input, isa, item) = item_fixture(
        r#"
enum Step { Retry(), Done(i64 value) }
i64 Loop() {
    while true {
        match Step::Retry() {
            Step::Done(value) => { return value; },
            Step::Retry() => { continue; },
        };
    }
}
"#,
    );
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a while-true loop whose body always returns or continues must terminate its final block");
    let clif = function.display().to_string();
    assert!(clif.contains("brif"), "{clif}");
    assert!(clif.contains("trap"), "the dead loop-exit block must be trapped as unreachable: {clif}");
}

#[test]
fn while_true_with_a_bare_return_body_terminates_its_final_block() {
    let (input, isa, item) = item_fixture("i64 Loop() { while true { return 1_i64; } }");
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a while-true loop with an always-returning body must terminate its final block");
    let clif = function.display().to_string();
    assert!(clif.contains("trap"), "the dead loop-exit block must be trapped as unreachable: {clif}");
}

#[test]
fn while_true_with_a_reachable_break_still_falls_through_normally() {
    let (input, isa, item) = item_fixture("i64 Loop() { while true { break; } return 9_i64; }");
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a while-true loop with a reachable break must lower through the following statement");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 9"), "{clif}");
}

#[test]
fn for_over_a_scalar_array_reads_the_header_length_and_indexes_by_stride() {
    let (input, isa, item) = item_fixture(
        "i64 Sum(i64[] values) { mut i64 total = 0_i64; for value in values { total = total + value; } return total; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).expect("a for over an i64[] parameter lowers");
    let clif = function.display().to_string();
    assert!(clif.contains("icmp ult"), "the index is compared unsigned against the header length:\n{clif}");
    assert!(clif.contains("imul_imm"), "an 8-byte element is addressed by its stride:\n{clif}");
    assert!(clif.contains("iadd_imm"), "the index advances by one per iteration:\n{clif}");
    assert!(clif.contains("trapz"), "a null array traps before the first element is read:\n{clif}");
    assert!(clif.contains("explicit_slot"), "the array value is rooted for the loop:\n{clif}");
}

#[test]
fn for_over_a_byte_array_addresses_elements_without_a_stride_multiply() {
    let (input, isa, item) =
        item_fixture("i64 Count(u8[] bytes) { mut i64 total = 0_i64; for byte in bytes { total = total + 1_i64; } return total; }");
    let function = emit_isle_item(&input, isa.as_ref(), item).expect("a for over a u8[] parameter lowers");
    let clif = function.display().to_string();
    assert!(clif.contains("icmp ult"), "{clif}");
    assert!(!clif.contains("imul_imm"), "a 1-byte element needs no stride multiply:\n{clif}");
}

#[test]
fn for_over_a_managed_array_roots_the_loop_variable() {
    let (input, isa, item) = item_fixture(
        "string Last(string[] names) { mut string last = \"\"; for name in names { last = name; } return last; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).expect("a for over a string[] parameter lowers");
    let clif = function.display().to_string();
    assert!(clif.contains("icmp ult"), "{clif}");
    assert!(
        clif.matches("explicit_slot").count() >= 2,
        "the array and the loop variable are rooted in stack slots for the loop:\n{clif}"
    );
    assert!(clif.contains("stack_store"), "each iteration publishes the element into the loop variable's root:\n{clif}");
}

#[test]
fn for_over_an_array_with_a_break_in_its_body_lowers() {
    let (input, isa, item) = item_fixture(
        "i64 Find(i64[] values) { mut i64 found = -1_i64; for value in values { if value == 3_i64 { found = value; break; } } return found; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).expect("a break inside an array for lowers");
    let clif = function.display().to_string();
    assert!(clif.contains("icmp ult"), "{clif}");
}
