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

fn calls(db: &dyn beskid_queries::Db, key: AstNodeKey, found: &mut Vec<AstNodeKey>) {
    if node_kind(db, key).ok().flatten() == Some(IndexedNodeKind::CallExpression) {
        found.push(key);
    }
    for child in child_nodes(db, key).ok().flatten().unwrap_or_default().iter() {
        calls(db, *child, found);
    }
}

#[test]
fn ordinary_mapping_lookalike_binds_both_contract_arguments_but_has_no_mapping_authority() {
    let source = r#"
        [Extern(Abi:"C", Library:"foreign")]
        contract Pair<S, D> { u64 Token(); }
        pub u64 Mapping<S, D>() { return Pair<S, D>.Token(); }
        pub u64 Entry() { return Mapping<i32, i64>(); }
    "#;
    let (db, _typed, root, _) = support::input_fixture_with_source(source);
    let mut found = Vec::new();
    calls(&db, root, &mut found);
    let mapping = found
        .iter()
        .filter_map(|call| beskid_queries::generic_call_specialization(&db, *call).ok().flatten())
        .find(|specialization| specialization.substitutions.len() == 2 && specialization.signature.parameters.is_empty())
        .expect("concrete Mapping<i32, i64> call");
    let instance = beskid_queries::generic_call_specialization_instance(&db, mapping).unwrap().unwrap();
    assert!(
        beskid_queries::dynamic_mapping_binding(&db, &instance).unwrap().is_none(),
        "a user-owned Mapping declaration never acquires canonical Dynamic mapping authority"
    );
    let bridge = found
        .iter()
        .filter_map(|call| beskid_queries::generic_call_specialization_in_environment(&db, *call, &instance).ok().flatten())
        .find(|bridge| bridge.declaration != instance.declaration)
        .expect("contract bridge specialized in the Mapping environment");
    let bound = bridge
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.to_string(), binding.argument))
        .collect::<Vec<_>>();
    assert_eq!(
        bound,
        vec![("S".to_owned(), beskid_queries::SemanticTypeId::I32), ("D".to_owned(), beskid_queries::SemanticTypeId::I64)]
    );
    assert_eq!(bridge.signature.result, beskid_queries::SemanticTypeId::U64);
}
