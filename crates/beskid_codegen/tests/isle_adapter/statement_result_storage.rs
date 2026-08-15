use super::support::{emit_isle_item, find_function_definition, item_fixture, item_fixture_with_root};

#[test]
fn nested_direct_call_results_lower_through_exact_statement_facts() {
    let (input, isa, item) = item_fixture(
        "i64 Count() { return 1_i64; } i64 Forward() { return Count(); } i64 Main() { i64 value = Forward(); return value; }",
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("nested direct-call result must lower through generation-bound statement facts");
    let clif = function.display().to_string();

    assert!(clif.contains("call"), "{clif}");
    assert!(!clif.contains("call_indirect"), "{clif}");
}

#[test]
fn inferred_let_results_lower_through_canonical_storage_facts() {
    let (input, isa, item) =
        item_fixture("i64 Count() { return 1_i64; } i64 Main() { let value = Count(); return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("inferred let storage must lower through its canonical declaration fact");
    let clif = function.display().to_string();

    assert!(clif.contains("call"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn scalar_match_results_lower_at_return_and_typed_storage_boundaries() {
    let (input, isa, item) = item_fixture(
        "enum Choice { First, Second } i64 Main(Choice choice) { i64 selected = match choice { Choice::First => 1_i64, Choice::Second => 2_i64, }; return match choice { Choice::First => selected, Choice::Second => 0_i64, }; }",
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("match results must lower through exact return and storage facts");
    let clif = function.display().to_string();

    assert!(clif.contains("br_table"), "{clif}");
    assert!(clif.contains("iconst.i64"), "{clif}");
}

#[test]
fn mutable_scalar_assignment_lowers_only_with_matching_storage_type() {
    let (input, isa, item) = item_fixture("i64 Main() { mut i64 value = 0_i64; value = 1_i64; return value; }");

    emit_isle_item(&input, isa.as_ref(), item)
        .expect("matching mutable-local storage must lower through its exact slot and ABI fact");
}

#[test]
fn immutable_assignment_fails_before_clif_emission() {
    let (input, isa, root) = item_fixture_with_root("i64 Main() { i64 value = 0_i64; value = 1_i64; return value; }");
    let item = find_function_definition(input.database(), root).expect("Main definition");

    let error = emit_isle_item(&input, isa.as_ref(), item)
        .expect_err("immutable assignment must not acquire storage authority");
    let rendered = error.display_with_db(input.database());

    assert!(rendered.contains("MissingRuleOrFact"), "{rendered}");
    assert!(rendered.contains("AssignExpression@"), "{rendered}");
}
