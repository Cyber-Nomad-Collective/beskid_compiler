//! Lambda environments, generic aggregate literals, and nominal receiver method calls.

use super::super::support::{
    NodeFacts, SyntaxModuleItem, call_lowering, find_call_expression, find_definition_of_kind,
    find_function_definition, find_function_definitions, find_node, find_nodes_of_kind, item_fixture_with_root,
    item_name, lower_syntax_program,
};
use cranelift_codegen::ir::types;

#[test]
fn captured_char_lambda_uses_i32_in_its_environment_and_target_signature() {
    let (input, isa, root) = item_fixture_with_root("i32 Main(char outer) { return (() => outer)(); }");
    let main = find_function_definition(input.database(), root).expect("application Main");

    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("captured char lambda lowers through the canonical ABI mapping");

    let lambda = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_lambda_entry_syntax"))
        .expect("lambda trampoline target");
    assert_eq!(lambda.function.signature.params[0].value_type, types::I64, "closure environment is a pointer");
    assert!(
        lambda.function.display().to_string().contains("load.i32"),
        "a captured char must retain the canonical i32 ABI representation"
    );
}

#[test]
fn parsed_syntax_program_omits_uncalled_generic_enum_declarations() {
    let (input, isa, root) = item_fixture_with_root(
        "type Box<T> { T value } enum Option<T> { Some(T value), None } i32 Main() { return 0; }",
    );
    let boxed = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::TypeDefinition)
        .expect("generic type declaration");
    let option = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumDefinition)
        .expect("generic enum declaration");
    let main = find_function_definitions(input.database(), root)[0];

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: boxed, symbol: "Box".into() },
            SyntaxModuleItem { key: option, symbol: "Option".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("generic declarations without executable bodies are omitted");

    assert_eq!(
        artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>(),
        ["Main"],
        "only executable syntax items enter the artifact",
    );
}

#[test]
fn nested_applied_generic_call_inside_an_aggregate_literal_lowers_from_one_environment() {
    let (input, isa, root) = item_fixture_with_root(
        "type Entry<TKey, TValue> { TKey key, TValue value } type Map<TKey, TValue> { Entry<TKey, TValue>[] entries, i64 count } T[] Identity<T>(T[] values) { return values; } Map<TKey, TValue> New<TKey, TValue>(Entry<TKey, TValue>[] empty) { return Map<TKey, TValue> { entries: Identity<Entry<TKey, TValue>>(empty), count: 0 }; } i64 Main(Entry<i64, string>[] empty) { Map<i64, string> map = New<i64, string>(empty); return map.count; }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("function name").to_string(),
        })
        .collect::<Vec<_>>();
    let new = items.iter().find(|item| item.symbol == "New").map(|item| item.key).expect("New function");
    let main = items.iter().find(|item| item.symbol == "Main").map(|item| item.key).expect("Main function");
    let call = find_call_expression(input.database(), main).expect("New call");
    let specialization = beskid_queries::generic_call_specialization(input.database(), call)
        .expect("New specialization query")
        .expect("New specialization");
    let instance = beskid_queries::generic_call_specialization_instance(input.database(), specialization)
        .expect("New instance query")
        .expect("New instance");
    let literal = find_node(input.database(), new, beskid_queries::IndexedNodeKind::StructLiteralExpression)
        .expect("Map literal");
    assert!(
        beskid_queries::aggregate_literal_specialization(input.database(), literal, instance.substitutions.clone(),)
            .expect("Map layout specialization query")
            .is_some(),
        "the enclosing source environment must materialize the Map layout",
    );
    assert!(
        input.aggregate_static_plan_for_specialization(literal, Some(&instance)).is_some(),
        "the specialized Map layout must produce one allocation plan",
    );
    let nested = find_call_expression(input.database(), new).expect("nested Identity<Entry<TKey, TValue>> call");
    assert!(
        beskid_queries::generic_call_specialization_in_environment(input.database(), nested, &instance)
            .expect("nested call environment query")
            .is_some(),
        "the nested call must consume the same recursive source environment",
    );

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("nested applied source arguments and their aggregate literal share one specialization environment");
}

#[test]
fn generic_aggregate_literal_keeps_declared_scalar_field_widths() {
    let (input, isa, root) = item_fixture_with_root(
        "type Queue<T> { T[] storage, i64 head, i64 count } T[] Identity<T>(T[] values) { return values; } Queue<T> New<T>(T[] empty) { return Queue<T> { storage: Identity<T>(empty), head: 0, count: 0 }; } i64 Main(string[] empty) { Queue<string> queue = New<string>(empty); return queue.count; }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("function name").to_string(),
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("generic aggregate specialization retains i64 fields beside pointer-shaped storage");
}

#[test]
fn generic_nominal_method_aggregate_literal_specializes_a_zero_argument_array_factory() {
    let (input, isa, root) = item_fixture_with_root(
        "type Queue<T> { T[] storage, i64 head, i64 count, Queue<T> Reset() { return Queue<T> { storage: Empty<T>(), head: 0, count: 0 }; } } T[] Empty<T>() { return []; } i64 Main(Queue<string> queue) { Queue<string> reset = queue.Reset(); return reset.count; }",
    );
    let db = input.database();
    let reset = find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("Queue.Reset method");
    let functions = find_function_definitions(db, root);
    let empty = functions
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Empty"))
        .expect("Empty factory");
    let main = functions
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let reset_call = find_call_expression(db, main).expect("queue.Reset call");
    let reset_specialization = beskid_queries::generic_call_specialization(db, reset_call)
        .expect("Reset specialization query")
        .expect("Queue<string>.Reset specialization");
    let reset_instance = beskid_queries::generic_call_specialization_instance(db, reset_specialization)
        .expect("Reset instance query")
        .expect("Queue<string>.Reset instance");
    let empty_call = find_call_expression(db, reset).expect("nested Empty<T> call");
    let empty_specialization =
        beskid_queries::generic_call_specialization_in_environment(db, empty_call, &reset_instance)
            .expect("nested Empty specialization query")
            .expect("nested Empty<string> specialization");
    assert_eq!(empty_specialization.declaration, empty, "the nested call must target the concrete Empty factory");
    assert!(empty_specialization.signature.parameters.is_empty());
    assert_eq!(empty_specialization.signature.result, beskid_queries::SemanticTypeId::POINTER);
    assert_eq!(empty_specialization.substitutions.len(), 1);
    assert_eq!(empty_specialization.substitutions[0].argument, beskid_queries::SemanticTypeId::STRING);

    let mut keys = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition);
    keys.extend(functions);
    let items = keys
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a generic nominal method must apply its receiver environment to a zero-argument array factory");
}

#[test]
fn unqualified_sibling_method_reuses_the_enclosing_nominal_specialization() {
    let (input, isa, root) = item_fixture_with_root(
        "type Set<T> { T[] values, bool Contains(T value) { return false; } Set<T> Add(T value) { if Contains(value) { return self; } return self; } } unit Main(Set<string> set) { set.Add(\"x\"); return; }",
    );
    let mut keys = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition);
    keys.extend(find_function_definitions(input.database(), root));
    let items = keys
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("an unqualified sibling method call consumes the current Set<string> environment");
}

#[test]
fn parsed_struct_literal_method_call_uses_receiver_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Point { i32 x, i32 Ping() { return 7; } } i32 Main() { return Point { x: 1 }.Ping(); }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main source item");
    let method =
        find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("inline method source item");
    assert_eq!(
        beskid_isle::syntax_types::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::MethodDefinition),
        beskid_isle::syntax_types::SyntaxNodeClassification::IsleLowered(beskid_isle::NodeKind::MethodDefinition),
        "MethodDefinition must be production-supported at the ISLE inventory boundary"
    );
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(
        facts.node_kind(method),
        Some(beskid_isle::NodeKind::MethodDefinition),
        "adapter must surface MethodDefinition as an IsleLowered item kind"
    );
    let call = find_call_expression(db, main).expect("method call syntax");
    let beskid_queries::CallLowering::Direct(declaration) =
        call_lowering(db, call).expect("method call query").expect("method call lowering")
    else {
        panic!("struct literal method call must resolve to its exact syntax declaration");
    };
    assert_eq!(declaration, method);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "Point_Ping".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("syntax-only module lowering supports the method receiver ABI");

    beskid_codegen::validate_artifact(&artifact).expect("method call imports the exact syntax method declaration");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_nominal_parameter_method_call_uses_receiver_abi_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Point { i32 x, i32 Ping() { return 7; } } i32 Main(Point point) { return point.Ping(); }",
    );
    let db = input.database();
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main source item");
    let method =
        find_node(db, root, beskid_queries::IndexedNodeKind::MethodDefinition).expect("inline method source item");
    let call = find_call_expression(db, main).expect("method call syntax");
    assert_eq!(call_lowering(db, call).expect("method call query"), Some(beskid_queries::CallLowering::Direct(method)));

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "Point_Ping".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("syntax-only module lowering supports an explicit nominal receiver ABI");

    beskid_codegen::validate_artifact(&artifact)
        .expect("nominal receiver call imports its exact syntax method declaration");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_program_specializes_an_inferred_generic_call_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Equal<T>(T actual, T expected, string because) { if actual == expected { return; } return; } unit Main() { Equal(\"same\", \"same\", \"because\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Equal".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("syntax module specializes inferred generic calls through exact ABI facts");

    beskid_codegen::validate_artifact(&artifact).expect("the generic call imports its specialized item identity");
    assert_eq!(artifact.functions.len(), 2);
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")),
        "generic source items must use a mangled specialization identity"
    );
}
