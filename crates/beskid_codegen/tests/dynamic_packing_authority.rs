//! A user-owned Pack declaration cannot issue canonical Dynamic representation facts.
#[path = "codegen_input/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;
use beskid_queries::{AstNodeKey, IndexedNodeKind, child_nodes, node_kind};

#[test]
fn ordinary_pack_lookalike_has_real_generic_facts_but_no_dynamic_authority() {
    let source = "pub T Pack<T>(T value) { return value; } pub i64 Entry() { return Pack<i64>(7); }";
    let (db, _typed, root, _) = support::input_fixture_with_source(source);
    fn call(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
        if node_kind(db, key).ok().flatten() == Some(IndexedNodeKind::CallExpression) {
            return Some(key);
        }
        child_nodes(db, key).ok().flatten()?.iter().find_map(|key| call(db, *key))
    }
    let call = call(&db, root).expect("registered actual call");
    let specialization = beskid_queries::generic_call_specialization(&db, call).unwrap().unwrap();
    let instance = beskid_queries::generic_call_specialization_instance(&db, specialization).unwrap().unwrap();
    assert_eq!(instance.substitutions.len(), 1);
    assert!(beskid_queries::dynamic_packing_shape(&db, &instance).unwrap().is_none());
}

#[test]
fn generic_extern_contract_binds_receiver_arguments_to_current_method() {
    let source = r#"
        [Extern(Abi:"C", Library:"foreign")]
        contract Bridge<T> { T Echo(T value); }
        pub i64 Entry() { return Bridge<i64>.Echo(7); }
    "#;
    let (db, _typed, root, _) = support::input_fixture_with_source(source);
    fn find_call(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
        if node_kind(db, key).ok().flatten() == Some(IndexedNodeKind::CallExpression) { return Some(key); }
        child_nodes(db, key).ok().flatten()?.iter().find_map(|child| find_call(db, *child))
    }
    let call = find_call(&db, root).expect("registered current contract call");
    let specialization = beskid_queries::generic_call_specialization(&db, call).unwrap().expect("contract specialization");
    assert_eq!(specialization.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
    assert_eq!(specialization.signature.result, beskid_queries::SemanticTypeId::I64);
    assert_eq!(specialization.substitutions.len(), 1);
    assert_eq!(specialization.substitutions[0].parameter.as_ref(), "T");
    assert_eq!(specialization.substitutions[0].argument, beskid_queries::SemanticTypeId::I64);
    let instance = beskid_queries::generic_call_specialization_instance(&db, specialization).unwrap().unwrap();
    assert!(beskid_queries::dynamic_packing_shape(&db, &instance).unwrap().is_none());
}
