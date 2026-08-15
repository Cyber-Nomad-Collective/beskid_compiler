use super::support::{assert_unavailable, key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{value_abi_type, SemanticTypeId, SyntaxGenerationId};

#[test]
fn statement_facts_prove_exact_direct_and_nested_call_results() {
    let source = "i64 Count() { return 1_i64; } i64 Forward() { return Count(); } unit Main() { i64 value = Forward(); value; return; }";
    let (db, _project, unit, generation, index) = setup(source);

    let returned_call = key(unit, generation, &index, NodeKind::ReturnStatement, 1);
    let typed_let = key(unit, generation, &index, NodeKind::LetStatement, 0);
    let expression_statement = key(unit, generation, &index, NodeKind::ExpressionStatement, 0);

    assert_eq!(value_abi_type(&db, returned_call).expect("return ABI fact"), Some(SemanticTypeId::I64));
    assert_eq!(value_abi_type(&db, typed_let).expect("storage ABI fact"), Some(SemanticTypeId::I64));
    assert_eq!(
        value_abi_type(&db, expression_statement).expect("expression statement ABI fact"),
        Some(SemanticTypeId::I64)
    );
}

#[test]
fn statement_facts_preserve_inferred_let_storage() {
    let source = "i64 Count() { return 1_i64; } i64 Main() { let value = Count(); return value; }";
    let (db, _project, unit, generation, index) = setup(source);
    let inferred_let = key(unit, generation, &index, NodeKind::LetStatement, 0);

    assert_eq!(value_abi_type(&db, inferred_let).expect("inferred storage ABI fact"), Some(SemanticTypeId::I64));
}

#[test]
fn statement_facts_prove_match_results_at_return_and_storage_boundaries() {
    let source = "enum Choice { First, Second } i64 Select(Choice choice) { return match choice { Choice::First => 1_i64, Choice::Second => 2_i64, }; } unit Main(Choice choice) { i64 selected = match choice { Choice::First => 3_i64, Choice::Second => 4_i64, }; return; }";
    let (db, _project, unit, generation, index) = setup(source);

    let returned_match = key(unit, generation, &index, NodeKind::ReturnStatement, 0);
    let stored_match = key(unit, generation, &index, NodeKind::LetStatement, 0);

    assert_eq!(value_abi_type(&db, returned_match).expect("match return ABI"), Some(SemanticTypeId::I64));
    assert_eq!(value_abi_type(&db, stored_match).expect("match storage ABI"), Some(SemanticTypeId::I64));
}

#[test]
fn mutable_assignment_requires_exact_storage_type_and_current_generation() {
    let source = "unit Main() { mut i64 value = 0_i64; value = 1_i64; return; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);

    assert_eq!(value_abi_type(&db, assignment).expect("mutable storage ABI"), Some(SemanticTypeId::I64));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "unit Main() { i64 replacement = 1_i64; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(value_abi_type(&db, assignment).expect("stale assignment ABI"), None);
}

#[test]
fn statement_facts_fail_closed_for_immutable_and_mismatched_storage() {
    let immutable = "unit Main() { i64 value = 0_i64; value = 1_i64; return; }";
    let (db, _project, unit, generation, index) = setup(immutable);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);
    assert_unavailable(value_abi_type(&db, assignment));

    let mismatched = "unit Main() { mut i64 value = 0_i64; value = true; return; }";
    let (db, _project, unit, generation, index) = setup(mismatched);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);
    assert_unavailable(value_abi_type(&db, assignment));
}
