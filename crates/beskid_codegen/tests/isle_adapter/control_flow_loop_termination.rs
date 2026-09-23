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
