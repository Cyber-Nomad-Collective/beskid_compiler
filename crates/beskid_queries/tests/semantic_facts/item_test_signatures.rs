use super::support::{assert_unavailable, key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, ItemSignature, SemanticTypeId, SyntaxGenerationId, item_body, item_signature, test_item,
};
use std::sync::Arc;

#[test]
fn item_signatures_cover_primitive_functions_methods_and_contracts() {
    let source = r#"
i64 Convert(i32 value, bool checked) { return value; }
type Counter { i64 value }
impl Counter { bool IsPositive(u8 threshold) { return true; } }
contract Converter { string Format(char value); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let contract = key(unit, generation, &index, NodeKind::ContractMethodSignature, 0);

    assert_eq!(
        item_signature(&db, function).expect("function signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::I32, beskid_queries::SemanticTypeId::BOOL,].into(),
            result: beskid_queries::SemanticTypeId::I64,
        })
    );
    assert_eq!(
        item_signature(&db, method).expect("method signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::U8].into(),
            result: beskid_queries::SemanticTypeId::BOOL,
        })
    );
    assert_eq!(
        item_signature(&db, contract).expect("contract signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::CHAR].into(),
            result: beskid_queries::SemanticTypeId::STRING,
        })
    );
}

#[test]
fn test_items_have_a_unit_signature_and_own_generation_safe_body_cursor() {
    let source = "test Smoke { return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let test = key(unit, generation, &index, NodeKind::TestDefinition, 0);

    assert_eq!(
        item_signature(&db, test).expect("test signature"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::UNIT })
    );
    assert_eq!(item_body(&db, test).expect("test body"), Some(test));
}

#[test]
fn test_item_facts_preserve_metadata_and_reject_stale_generations() {
    let source = r#"test Smoke {
        meta { group = "fast"; tags = "unit, smoke"; }
        skip { condition = true; reason = "not on this host"; }
        return;
    }"#;
    let (db, _project, unit, generation, index) = setup(source);
    let test = key(unit, generation, &index, NodeKind::TestDefinition, 0);

    let facts = test_item(&db, test).expect("test facts query").expect("current test facts");
    assert_eq!(facts.name.as_ref(), "Smoke");
    assert_eq!(facts.qualified_name.as_ref(), "Smoke");
    assert_eq!(facts.group.as_deref(), Some("fast"));
    assert_eq!(facts.tags.iter().map(|tag| tag.as_ref()).collect::<Vec<_>>(), ["unit", "smoke"]);
    assert_eq!(facts.skip_condition, Some(true));
    assert_eq!(facts.skip_reason.as_deref(), Some("not on this host"));
    assert_eq!(
        test_item(&db, AstNodeKey { generation: SyntaxGenerationId(generation.0 - 1), ..test })
            .expect("stale test facts"),
        None
    );
}

#[test]
fn item_signature_does_not_guess_complex_type_identity() {
    let source = "Value Identity(Value value) { return value; }";
    let (db, _project, unit, generation, index) = setup(source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    assert_unavailable(item_signature(&db, function));
}
