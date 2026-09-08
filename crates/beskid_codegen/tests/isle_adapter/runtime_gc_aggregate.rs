use super::support::*;

#[test]
fn specialized_map_descriptor_traces_its_array_field() {
    let (input, _isa, root) = item_fixture_with_root(
        "type Entry<TKey, TValue> { TKey key, TValue value } type Map<TKey, TValue> { Entry<TKey, TValue>[] entries, i64 count, Map<TKey, TValue> Insert(TKey key, TValue value) { Entry<TKey, TValue> fresh = Entry<TKey, TValue> { key: key, value: value }; return Map<TKey, TValue> { entries: entries, count: count + 1 }; } } i64 Main(Map<string, string> map) { Map<string, string> next = map.Insert(\"key\", \"value\"); return next.count; }",
    );
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let insert = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("Insert method");
    let main = functions
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let call = find_call_expression(db, main).expect("Map<i64, i64>.Insert call");
    let specialization = beskid_queries::generic_call_specialization(db, call)
        .expect("specialization query")
        .expect("Insert specialization");
    let instance = beskid_queries::generic_call_specialization_instance(db, specialization)
        .expect("specialization instance query")
        .expect("Insert instance");
    let plans = find_nodes_of_kind(db, insert, beskid_queries::IndexedNodeKind::StructLiteralExpression)
        .into_iter()
        .map(|literal| {
            input
                .aggregate_static_plan_for_specialization(literal, Some(&instance))
                .expect("specialized aggregate allocation plan")
        })
        .collect::<Vec<_>>();

    assert!(
        plans.iter().any(|plan| plan.pointer_map_offsets.as_ref() == [16]),
        "Map.entries must be traced from the object payload: {plans:?}"
    );
    assert!(
        plans.iter().any(|plan| plan.pointer_map_offsets.as_ref() == [16, 24]),
        "MapEntry<string, string> must trace both pointer fields: {plans:?}"
    );
}
