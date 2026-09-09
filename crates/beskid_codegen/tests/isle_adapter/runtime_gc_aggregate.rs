use super::support::*;

#[test]
fn specialized_map_descriptor_traces_its_array_field() {
    let (input, _isa, root) = item_fixture_with_root(
        "type Entry<TKey, TValue> { TKey key, TValue value } type Map<TKey, TValue> { Entry<TKey, TValue>[] entries, i64 count, Map<TKey, TValue> Insert(TKey key, TValue value) { Entry<TKey, TValue> fresh = Entry<TKey, TValue> { key: key, value: value }; return Map<TKey, TValue> { entries: entries, count: count + 1 }; } } i64 Main(Map<string, string> map) { Map<string, string> next = map.Insert(\"key\", \"value\"); return next.count; }",
    );
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let insert = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("Insert method");
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

#[test]
fn specialized_generic_string_field_uses_content_equality() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type Entry<T> { T key }
type Map<T> {
    Entry<T> entry,
    bool Contains(T key) {
        Entry<T> current = entry;
        return current.key == key;
    }
}
bool Main(Map<string> map) { return map.Contains("key"); }
"#,
    );
    let db = input.database();
    let contains = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("Map.Contains method");
    let main = find_function_definition(db, root).expect("Main function");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: contains, symbol: "Map_Contains".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("Map<string>.Contains lowers through its exact specialization");
    let contains = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Map_Contains#generic_"))
        .expect("specialized Map<string>.Contains function");
    let clif = contains.function.display().to_string();

    assert!(clif.contains(" = %str_eq "), "string field equality must import str_eq:\n{clif}");
    assert!(clif.lines().any(|line| line.contains("= call ")), "string field equality must call str_eq:\n{clif}");
}

#[test]
fn specialized_nested_generic_call_result_field_lowers() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type Entry<T> { T key }
T Fetch<T>(T[] entries, i64 index) { return entries[index]; }
type Map<T> {
    Entry<T>[] entries,
    bool Contains(T key) { return Fetch<Entry<T>>(entries, 0_i64).key == key; }
}
bool Main(Map<string> map) { return map.Contains("key"); }
"#,
    );
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let fetch = functions
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Fetch"))
        .expect("Fetch function");
    let main = functions
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let contains = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("Map.Contains method");
    let member = find_node(db, contains, beskid_queries::IndexedNodeKind::MemberExpression)
        .expect("call-result Entry<T>.key projection");
    let nested_call =
        find_node(db, member, beskid_queries::IndexedNodeKind::CallExpression).expect("nested Fetch<Entry<T>> call");
    let main_call = find_call_expression(db, main).expect("Map<string>.Contains call");
    let specialization = beskid_queries::generic_call_specialization(db, main_call)
        .expect("specialization query")
        .expect("Contains specialization");
    let instance = beskid_queries::generic_call_specialization_instance(db, specialization)
        .expect("specialization instance query")
        .expect("Contains instance");
    beskid_queries::generic_call_specialization_in_environment(db, nested_call, &instance)
        .expect("nested specialization query")
        .expect("nested Fetch<Entry<string>> specialization");
    beskid_queries::aggregate_field_access_specialization(db, member, &instance)
        .expect("field access query")
        .expect("nested generic call-result field access");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: fetch, symbol: "Fetch".into() },
            SyntaxModuleItem { key: contains, symbol: "Map_Contains".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a nested generic call result retains its nominal field layout");
    let contains = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Map_Contains#generic_"))
        .expect("specialized Map<string>.Contains function");
    let clif = contains.function.display().to_string();
    assert!(clif.contains("load.i64"), "the Entry<string>.key field must be loaded:\n{clif}");
    assert!(clif.contains(" = %str_eq "), "the loaded string key must use content equality:\n{clif}");
}
