use super::support::{assert_unavailable, key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, GenericSpecializationInstance, ItemSignature, SemanticTypeId, SourceUnitId,
    SyntaxGenerationId, abi_type, generic_specialization_identity, literal_fact, node_type,
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
fn primitive_numeric_conversion_uses_the_declared_integer_constant_value_type() {
    let source = "const STATUS = 2; i32 Main() { return i32(STATUS); }";
    let (db, _project, unit, generation, index) = setup(source);
    let conversion = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        primitive_numeric_conversion(&db, conversion).expect("constant conversion fact"),
        Some(beskid_queries::PrimitiveNumericConversion { from: SemanticTypeId::WORD, to: SemanticTypeId::I32 })
    );
}

#[test]
fn declared_integer_constant_has_word_abi_type() {
    let source = "const STATUS = 2; word Main() { return STATUS; }";
    let (db, _project, unit, generation, index) = setup(source);
    let constant = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(abi_type(&db, constant).expect("constant ABI type"), Some(SemanticTypeId::WORD));
}

/// A module constant use has its declared literal's natural type: the type a literal names is
/// never narrower than the value it holds (the `LIMIT_*` runtime constants are words, not `i32`).
#[test]
fn declared_integer_constant_node_type_is_its_literal_natural_type() {
    let source = "const SMALL = 7; const BIG = 3000000000; const LIMIT_INVALID = 0xFFFFFFFFFFFFFFFE; \
                  word Main(word scaled, i64 wide, i32 narrow) { if wide == BIG || narrow == SMALL { return 0; } return LIMIT_INVALID; }";
    let (db, _project, unit, generation, index) = setup(source);
    let path = |ordinal| key(unit, generation, &index, NodeKind::PathExpression, ordinal);
    let typed = |ordinal| node_type(&db, path(ordinal)).expect("constant node type");

    // Path expressions in source order: wide, BIG, narrow, SMALL, LIMIT_INVALID.
    assert_eq!(typed(1), Some(SemanticTypeId::I64));
    assert_eq!(typed(3), Some(SemanticTypeId::I32));
    assert_eq!(typed(4), Some(SemanticTypeId::WORD));
}

/// An unsuffixed literal outside `i32` has the `i64` natural type its value needs, so an inferred
/// `let` never stores it truncated to 32 bits.
#[test]
fn unsuffixed_literal_beyond_i32_has_the_i64_natural_type() {
    let source = "i64 Main() { let x = 3000000000; let y = 7; let z = 0x1FFFFFFFF; return x; }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = |ordinal| key(unit, generation, &index, NodeKind::Literal, ordinal);

    assert_eq!(node_type(&db, literal(0)).expect("wide literal type"), Some(SemanticTypeId::I64));
    assert_eq!(node_type(&db, literal(1)).expect("narrow literal type"), Some(SemanticTypeId::I32));
    assert_eq!(node_type(&db, literal(2)).expect("wide hex literal type"), Some(SemanticTypeId::I64));
    assert_eq!(abi_type(&db, literal(0)).expect("wide literal ABI"), Some(SemanticTypeId::I64));
}

#[test]
fn try_expression_fact_resolves_result_payload_and_enclosing_error_return() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i32, Error> Main(Result<i32, Error> value) { return value?; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);

    let fact = try_expression_fact(&db, expression).expect("try expression query").expect("try fact");
    assert_eq!(fact.expression, expression);
    assert_eq!(fact.operand, key(unit, generation, &index, NodeKind::PathExpression, 0));
    assert_eq!(fact.payload_type, SemanticTypeId::I32);
    assert_eq!(fact.error_type, SemanticTypeId::POINTER);
    assert_eq!(fact.enclosing_return, SemanticTypeId::POINTER);
    assert_eq!(fact.operand_layout, fact.return_layout);
}

#[test]
fn try_expression_fact_accepts_distinct_success_with_exact_error_identity() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<i64, Error> Main(Result<i32, Error> value) { return value?; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);

    let fact = try_expression_fact(&db, expression).expect("compatible propagation").expect("try fact");
    assert_ne!(fact.operand_layout, fact.return_layout);
}

#[test]
fn try_expression_fact_fails_closed_for_associated_result_payloads() {
    let source = "enum Error { Failed() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } Result<Iterator::Item, Error> Main(Result<Iterator::Item, Error> value) { return value?; }";
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
fn generic_specialization_identity_is_reproducible_across_syntax_generations() {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Generic.bd"));
    let instance = |generation| GenericSpecializationInstance {
        contract_witnesses: Arc::from([]),
        declaration: AstNodeKey {
            unit,
            generation: SyntaxGenerationId(generation),
            node: beskid_analysis::syntax::AstNodeId(7),
        },
        declaration_identity: Arc::from("Core::Identity"),
        signature: ItemSignature { parameters: Arc::from([SemanticTypeId::I64]), result: SemanticTypeId::I64 },
        substitutions: Arc::from([]),
    };

    let low = generic_specialization_identity(&instance(1));
    let high = generic_specialization_identity(&instance((1_u64 << 32) | 1));

    assert_eq!(low, high, "process-local syntax generations must not leak into emitted module identity");

    let mut distinct = instance(1);
    distinct.declaration_identity = Arc::from("Core::DistinctIdentity");
    assert_ne!(low, generic_specialization_identity(&distinct), "distinct declarations must not alias");
}
