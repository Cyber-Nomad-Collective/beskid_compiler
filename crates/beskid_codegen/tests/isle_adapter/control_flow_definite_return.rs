//! Agreement of the definite-return check (E1258) with lowering: every valued body the legality
//! gate accepts ends its final block, so lowering never reports "generated statement body did
//! not terminate its final block" (E2103) for it.

use super::support::{emit_isle_item, item_fixture};

fn assert_accepted_body_lowers(source: &str) {
    let (input, isa, item) = item_fixture(source);
    let findings = beskid_queries::check_items(input.database(), &[item]).err().unwrap_or_default();
    assert!(
        !findings.iter().any(|finding| finding.kind.code() == "E1258"),
        "the definite-return check must accept {source}: {findings:?}"
    );
    if let Err(error) = emit_isle_item(&input, isa.as_ref(), item) {
        panic!("an accepted valued body must lower: {source}: {error}");
    }
}

#[test]
fn every_body_the_definite_return_check_accepts_lowers() {
    for source in [
        "i32 F(bool c) { if (c) { return 1; } else { return 2; } }",
        "i32 F(bool c, bool d) { if (c) { return 1; } else if (d) { return 2; } else { return 3; } }",
        "i32 F() { while (true) { } }",
        "i32 F(i32 x) { while (true) { if (x > 0) { return x; } } }",
        "i32 F() { while (true) { while (true) { break; } } }",
        "enum Step { Stop(), Go(i32 value) } \
         i32 F(Step s) { match s { Step::Stop() => { return 0; }, Step::Go(v) => { return v; }, } }",
        "i32 F(i32 x) { match x { 1 => { return 1; }, _ => { return 0; }, } }",
        "i32 F() { { return 1; }; }",
        "i32 F(bool c) { { if (c) { return 1; } }; return 2; }",
        "i32 F() { Stop(); } never Stop() { while (true) { } }",
    ] {
        assert_accepted_body_lowers(source);
    }
}

#[test]
fn a_body_that_falls_off_its_end_is_rejected_before_lowering() {
    let (input, _isa, item) = item_fixture("i32 Main() { 0; }");
    let findings = beskid_queries::check_items(input.database(), &[item]).expect_err("the gate rejects Main");
    assert_eq!(findings.iter().map(|finding| finding.kind.code()).collect::<Vec<_>>(), ["E1258"]);
}
