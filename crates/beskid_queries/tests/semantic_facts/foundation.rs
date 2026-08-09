use super::support::{assert_unavailable, key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, GenericSpecializationInstance, ItemSignature, SemanticTypeId, SourceUnitId,
    SyntaxGenerationId, TryExpressionFact, abi_type, generic_specialization_identity, literal_fact, node_type,
    primitive_numeric_conversion, try_expression_fact,
};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn warm_point_query_uses_registered_expanded_syntax_without_reparse() {
    let (mut db, _project, unit, generation, index) = setup("i32 Main() { return 7; }");
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    assert!(literal_fact(&db, literal).expect("cold literal").is_some());
    assert_eq!(node_type(&db, literal).expect("cold type"), Some(beskid_queries::SemanticTypeId::I32));

    db.ensure_file_text(unit.path(&db).clone(), "this is deliberately invalid Beskid source".to_string());
    assert!(literal_fact(&db, literal).expect("warm literal").is_some());
    assert_eq!(node_type(&db, literal).expect("warm type"), Some(beskid_queries::SemanticTypeId::I32));
    assert_eq!(db.syntax_authority_counts(), (1, 1));
}

#[test]
fn primitive_numeric_conversion_call_has_a_typed_result_without_dynamic_dispatch() {
    let (db, _project, unit, generation, index) = setup("i64 Main(word index) { return i64(index); }");
    let conversion = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(node_type(&db, conversion).expect("conversion type"), Some(SemanticTypeId::I64));
    assert_eq!(abi_type(&db, conversion).expect("conversion ABI type"), Some(SemanticTypeId::I64));
    assert_eq!(
        primitive_numeric_conversion(&db, conversion).expect("conversion fact"),
        Some(beskid_queries::PrimitiveNumericConversion { from: SemanticTypeId::WORD, to: SemanticTypeId::I64 })
    );
}

#[test]
fn try_expression_fact_resolves_result_payload_and_enclosing_error_return() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i32, Error> Main(Result<i32, Error> value) { return value?; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);

    assert_eq!(
        try_expression_fact(&db, expression).expect("try expression query"),
        Some(TryExpressionFact {
            expression,
            operand: key(unit, generation, &index, NodeKind::PathExpression, 0),
            payload_type: SemanticTypeId::I32,
            error_type: SemanticTypeId::POINTER,
            enclosing_return: SemanticTypeId::POINTER,
        })
    );
}

#[test]
fn try_expression_fact_rejects_differing_result_payload_instantiation() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i64, Error> Main(Result<i32, Error> value) { return value?; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);

    assert_unavailable(try_expression_fact(&db, expression));
}

#[test]
fn try_expression_fact_rejects_result_lookalike_without_the_canonical_error_variant() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Err(TError error) } Result<i32, Error> Main(Result<i32, Error> value) { return value?; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);

    assert_unavailable(try_expression_fact(&db, expression));
}

#[test]
fn try_expression_fact_rejects_result_with_swapped_payload_and_error_representations() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TError value), Error(TValue error) } Result<i32, Error> Main(Result<i32, Error> value) { return value?; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);

    assert_unavailable(try_expression_fact(&db, expression));
}

#[test]
fn generic_specialization_identity_distinguishes_high_generation_bits() {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Generic.bd"));
    let instance = |generation| GenericSpecializationInstance {
        declaration: AstNodeKey {
            unit,
            generation: SyntaxGenerationId(generation),
            node: beskid_analysis::syntax::AstNodeId(7),
        },
        signature: ItemSignature { parameters: Arc::from([SemanticTypeId::I64]), result: SemanticTypeId::I64 },
        substitutions: Arc::from([]),
    };

    let low = generic_specialization_identity(&instance(1));
    let high = generic_specialization_identity(&instance((1_u64 << 32) | 1));

    assert_ne!(low, high, "distinct syntax generations must not share a specialized module identity");
}
