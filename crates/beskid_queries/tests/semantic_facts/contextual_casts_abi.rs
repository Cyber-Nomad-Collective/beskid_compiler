use super::support::{assert_unavailable, key, key_at_start, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    SemanticTypeId, SyntaxGenerationId, abi_type, call_argument_abi_type, cast_intents,
    contextual_integer_literal_abi_type, primitive_numeric_conversion, value_abi_type,
};
use std::sync::Arc;

#[test]
fn declared_integer_constant_uses_mutable_assignment_storage_abi() {
    let source = "const DEFAULT_CAPACITY = 16; i64 Main(mut i64 capacity) { capacity = DEFAULT_CAPACITY; return capacity; }";
    let (db, _project, unit, generation, index) = setup(source);
    let constant = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.rfind("DEFAULT_CAPACITY").expect("constant use"),
    );

    assert_eq!(
        contextual_integer_literal_abi_type(&db, constant).expect("contextual constant ABI"),
        Some(SemanticTypeId::I64)
    );
    assert_eq!(value_abi_type(&db, constant).expect("constant value ABI"), Some(SemanticTypeId::I64));
}

#[test]
fn cast_intents_use_exact_typed_let_constraints_and_local_types() {
    let source = r#"unit Main() {
    i64 widenedLiteral = 1;
    i32 source = 2;
    i64 widenedLocal = source;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    let source_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let expected = Arc::from([beskid_queries::CastIntent {
        from: beskid_queries::SemanticTypeId::I32,
        to: beskid_queries::SemanticTypeId::I64,
    }]);
    assert_eq!(cast_intents(&db, literal).expect("literal cast"), Some(Arc::clone(&expected)));
    assert_eq!(cast_intents(&db, source_reference).expect("local cast"), Some(expected));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "unit Main() { i64 replacement = 1_i64; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(cast_intents(&db, literal).expect("stale cast"), None);
}

#[test]
fn cast_intents_use_manifest_runtime_parameter_types_for_contextual_literals() {
    let source = "pointer Main(pointer state) { return pointer_add(state, 8); }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);

    assert_eq!(
        cast_intents(&db, literal).expect("runtime argument cast"),
        Some(Arc::from([beskid_queries::CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::WORD,
        }]))
    );
}

#[test]
fn cast_intents_use_binary_operand_types_for_contextual_literals() {
    let source = "bool Main(word size) { return size < 16; }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);

    assert_eq!(
        cast_intents(&db, literal).expect("binary operand cast"),
        Some(Arc::from([beskid_queries::CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::WORD,
        }]))
    );
}

#[test]
fn cast_intents_keep_nested_call_literals_bound_to_the_parameter_type() {
    let source = r#"
pointer NativePointer(word value) { return value; }
bool Main(pointer object) { return object == NativePointer(0); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);

    assert_eq!(
        cast_intents(&db, literal).expect("nested call argument cast"),
        Some(Arc::from([beskid_queries::CastIntent {
            from: beskid_queries::SemanticTypeId::I32,
            to: beskid_queries::SemanticTypeId::WORD,
        }]))
    );
}

#[test]
fn binary_operand_abi_type_does_not_cross_explicit_numeric_conversion_boundary() {
    let source = "u8 Main(u8 b) { return b - u8(97); }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    let conversion = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_unavailable(beskid_queries::binary_operand_abi_type(&db, literal));
    assert_unavailable(call_argument_abi_type(&db, literal));
    assert_eq!(cast_intents(&db, literal).expect("conversion argument cast intents"), None);
    assert_eq!(abi_type(&db, literal).expect("literal ABI type"), Some(SemanticTypeId::I32));
    assert_eq!(
        primitive_numeric_conversion(&db, conversion).expect("conversion fact"),
        Some(beskid_queries::PrimitiveNumericConversion { from: SemanticTypeId::I32, to: SemanticTypeId::U8 })
    );
}

#[test]
fn value_abi_type_preserves_a_direct_call_result_at_a_declared_storage_boundary() {
    let source = "i64 Count() { return 1_i64; } unit Main() { i64 value = Count(); return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let local = key(unit, generation, &index, NodeKind::LetStatement, 0);

    assert_eq!(value_abi_type(&db, call).expect("direct call value ABI"), Some(SemanticTypeId::I64));
    assert_eq!(value_abi_type(&db, local).expect("declared storage ABI"), Some(SemanticTypeId::I64));
}
