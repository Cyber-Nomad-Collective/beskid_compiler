use super::support::{
    NodeFacts, SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, format_ast_node_site,
    item_fixture_with_root, lower_syntax_program,
};

#[test]
fn event_fields_have_zeroed_physical_slots_without_shifting_value_fields() {
    let source = r#"type EventOnly { event{2} First(), event Removed() }
type Mixed { i32 count, event{4} Changed(string payload), i64 total, event Removed() }
unit Main() {
    EventOnly first = EventOnly { };
    EventOnly second = EventOnly { };
    Mixed mixed = Mixed { count: 1, total: 2_i64 };
    return;
}
"#;
    let (input, _isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let declarations = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::TypeDefinition);
    let event_only = declarations[0];
    let mixed = declarations[1];
    let event_only_layout = input.aggregate_object_layout(event_only).expect("physical EventOnly layout");
    let mixed_layout = input.aggregate_object_layout(mixed).expect("physical Mixed layout");

    // The managed allocator clears the whole object before publishing it; the event slots need
    // only be reserved and included in the descriptor's pointer map, not explicitly stored.
    assert_eq!(event_only_layout.object_size, 32, "two null-initialized event slots follow the 16-byte header");
    assert_eq!(event_only_layout.pointer_map_offsets.as_ref(), &[16, 24]);
    assert_eq!(event_only_layout.fields.len(), 0, "events are not readable value fields");
    assert_eq!(mixed_layout.fields.iter().map(|field| field.field_offset).collect::<Vec<_>>(), [16, 24]);
    assert_eq!(mixed_layout.object_size, 48, "event slots append after ordinary value storage");
    assert_eq!(mixed_layout.pointer_map_offsets.as_ref(), &[32, 40]);

    let event_layouts = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::Field)
        .into_iter()
        .filter_map(|member| beskid_queries::event_field_layout(db, member).ok().flatten())
        .collect::<Vec<_>>();
    assert_eq!(
        event_layouts.iter().map(|event| event.slot_offset).collect::<Vec<_>>(),
        [16, 24, 32, 40],
        "the generation-bound fact owns each source event slot"
    );
    assert_eq!(event_layouts.iter().map(|event| event.capacity).collect::<Vec<_>>(), [2, 0, 4, 0]);
    assert!(event_layouts.iter().all(|event| event.owner_type == event_only || event.owner_type == mixed));

    let literals = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::StructLiteralExpression);
    let event_literals = literals
        .iter()
        .copied()
        .filter(|literal| {
            beskid_queries::aggregate_literal_declaration(db, *literal).ok().flatten() == Some(event_only)
        })
        .collect::<Vec<_>>();
    assert_eq!(event_literals.len(), 2);
    let plans = event_literals
        .iter()
        .map(|literal| input.aggregate_static_plan(*literal).expect("event allocation plan"))
        .collect::<Vec<_>>();
    assert_eq!(plans[0].object_size, plans[1].object_size);
    assert_ne!(plans[0].allocation_request_symbol, plans[1].allocation_request_symbol);
    assert_eq!(plans[0].pointer_map_offsets.as_ref(), &[16, 24]);
}

#[test]
fn event_assignments_have_generation_bound_subscribe_and_first_unsubscribe_facts() {
    let (input, _isa, root) = item_fixture_with_root(EVENT_SOURCE);
    let db = input.database();
    let operations = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::AssignExpression)
        .into_iter()
        .filter_map(|key| {
            let site = format_ast_node_site(db, key);
            (site.contains("AssignExpression@5:") || site.contains("AssignExpression@6:"))
                .then(|| beskid_queries::event_operation(db, key).expect("event operation query").expect("event fact"))
        })
        .collect::<Vec<_>>();

    assert_eq!(operations.len(), 2);
    assert_eq!(operations[0].operation, beskid_queries::EventOperationKind::Subscribe);
    assert_eq!(operations[1].operation, beskid_queries::EventOperationKind::UnsubscribeFirst);
    assert_eq!(operations[0].capacity, 4);
    assert_eq!(operations[0].slot_offset, 16);
    assert_eq!(operations[0].declaration, operations[1].declaration);
    assert_eq!(operations[0].field, operations[1].field);
    assert_eq!(operations[0].receiver, operations[1].receiver);
    assert!(operations[0].handler.is_some());
    assert!(operations[1].handler.is_some());
    assert_eq!(operations[0].delegate_signature, operations[1].delegate_signature);
    let subscribe_handler = beskid_queries::resolved_local(db, operations[0].handler.expect("handler"))
        .expect("handler resolution query")
        .expect("subscribe handler resolves to local");
    let unsubscribe_handler = beskid_queries::resolved_local(db, operations[1].handler.expect("handler"))
        .expect("handler resolution query")
        .expect("unsubscribe handler resolves to local");
    assert_eq!(subscribe_handler.declaration, unsubscribe_handler.declaration);

    let raise = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .find(|key| format_ast_node_site(db, *key).contains("CallExpression@7:"))
        .expect("event raise call");
    let raise = beskid_queries::event_operation(db, raise).expect("raise query").expect("raise fact");
    assert_eq!(raise.operation, beskid_queries::EventOperationKind::Raise);
    assert_eq!(raise.arguments.len(), 1);
    assert_eq!(
        raise.delegate_signature.as_ref().expect("delegate signature").parameters.as_ref(),
        &[beskid_queries::SemanticTypeId::STRING]
    );

    let facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new(&input);
    assert_eq!(
        facts.event_operation(operations[0].operation_node).expect("subscribe plan").operation,
        beskid_isle::EventOperation::Subscribe
    );
    assert_eq!(
        facts.event_operation(operations[1].operation_node).expect("unsubscribe plan").operation,
        beskid_isle::EventOperation::UnsubscribeFirst
    );
    assert_eq!(
        facts.event_operation(raise.operation_node).expect("raise plan").operation,
        beskid_isle::EventOperation::Raise
    );
    assert_eq!(facts.assignment_kind(operations[0].operation_node), Some(beskid_isle::AssignmentKind::EventSubscribe));
    assert_eq!(
        facts.assignment_kind(operations[1].operation_node),
        Some(beskid_isle::AssignmentKind::EventUnsubscribeFirst)
    );
}

#[test]
fn event_handler_bindings_and_event_assignments_have_managed_pointer_abi_facts() {
    let source = r#"type User { event{4} created(string payload) }
impl User { unit Fire() { this.created("payload"); } }
unit Main() {
    User u = User { };
    unit(string) ordinary = (string payload) => { };
    unit(string) handler = (string payload) => { };
    u.created += handler;
    u.created -= handler;
    u.Fire();
    return;
}
"#;
    let (input, _isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let assignments = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::AssignExpression);
    let event_operations = assignments
        .into_iter()
        .filter_map(|key| beskid_queries::event_operation(db, key).expect("event operation query"))
        .collect::<Vec<_>>();
    assert_eq!(event_operations.len(), 2);

    let event_lambdas = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)
        .into_iter()
        .filter_map(|lambda| {
            beskid_queries::event_handler_lambda_for_local(db, lambda)
                .expect("event lambda query")
                .map(|fact| fact.lambda)
        })
        .collect::<Vec<_>>();
    assert_eq!(event_lambdas.len(), 1, "only the exact event-used lambda receives the event signature path");

    let binding = event_operations[0].handler.expect("handler path");
    let declaration = beskid_queries::resolved_local(db, binding)
        .expect("handler resolution query")
        .expect("handler resolves to local")
        .declaration;
    assert_eq!(
        beskid_queries::value_abi_type(db, declaration).expect("handler local ABI query"),
        Some(beskid_queries::SemanticTypeId::POINTER),
        "event-used lambda locals store the managed handler wrapper pointer"
    );
    for operation in event_operations {
        assert_eq!(
            beskid_queries::value_abi_type(db, operation.operation_node).expect("event assignment ABI query"),
            Some(beskid_queries::SemanticTypeId::POINTER),
            "event compound assignments carry the stable managed handler identity, not a runtime count"
        );
    }
    let raise = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .find(|key| beskid_queries::event_operation(db, *key).expect("event raise query").is_some())
        .expect("event raise call");
    assert_eq!(
        beskid_queries::value_abi_type(db, raise).expect("event raise ABI query"),
        Some(beskid_queries::SemanticTypeId::UNIT),
        "event raise is a unit statement, not an ordinary dynamic-call result"
    );
}

#[test]
fn event_operation_selector_does_not_claim_value_field_assignments_or_event_reads() {
    let source = r#"type User { event{4} Created(string payload), i32 Count }
unit Main() {
    User u = User { Count: 1 };
    u.Count += 1;
    return;
}

"#;
    let (input, _isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new(&input);
    let assignments = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::AssignExpression);
    assert_eq!(assignments.len(), 1);
    assert_eq!(facts.assignment_kind(assignments[0]), Some(beskid_isle::AssignmentKind::Field));
    assert!(
        facts.event_operation(assignments[0]).is_none(),
        "value-field compound assignments are not event operations"
    );

    let (event_input, _isa, event_root) = item_fixture_with_root(
        "type User { event{4} Created(string payload) } unit Main(User u) { u.Created(\"x\"); return; }",
    );
    let event_db = event_input.database();
    let event_facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new(&event_input);
    let member_paths = find_nodes_of_kind(event_db, event_root, beskid_queries::IndexedNodeKind::PathExpression);
    assert!(member_paths.iter().all(|key| event_facts.event_operation(*key).is_none()));
}

#[test]
fn event_selector_rejects_stale_keys_and_does_not_lower_unbounded_declarations_as_capacity_zero() {
    let (input, _isa, root) = item_fixture_with_root(EVENT_SOURCE);
    let db = input.database();
    let operation = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::AssignExpression)
        .into_iter()
        .find(|key| format_ast_node_site(db, *key).contains("AssignExpression@5:"))
        .expect("subscribe source node");
    let facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new(&input);
    let stale =
        beskid_queries::AstNodeKey { generation: beskid_analysis::syntax::SyntaxGenerationId(u64::MAX), ..operation };
    assert!(facts.event_operation(stale).is_none(), "stale generations do not select the event rule");

    let (unbounded_input, _isa, unbounded_root) = item_fixture_with_root(
        "type User { event Changed() } unit Main(User u) { unit() handler = () => { return; }; u.Changed += handler; return; }",
    );
    let unbounded_db = unbounded_input.database();
    let unbounded_operation =
        find_nodes_of_kind(unbounded_db, unbounded_root, beskid_queries::IndexedNodeKind::AssignExpression)
            .into_iter()
            .next()
            .expect("unbounded subscription syntax");
    let fact = beskid_queries::event_operation(unbounded_db, unbounded_operation)
        .expect("event query")
        .expect("declaration and handler are semantically resolved");
    assert_eq!(fact.capacity, 0, "zero is only the query's absent-capacity sentinel");
    let unbounded_facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new(&unbounded_input);
    assert!(
        unbounded_facts.event_operation(unbounded_operation).is_none(),
        "the unresolved v0.5 default must not be encoded as an invalid capacity-zero ABI call"
    );
}

#[test]
fn event_subscribe_and_unsubscribe_import_manifest_services_and_use_the_declared_capacity() {
    let source = r#"type User { event{4} Created(string payload) }
unit Main() {
    User u = User { };
    unit(string) boom = (string payload) => { return; };
    u.Created += boom;
    u.Created -= boom;
    return;
}

"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new_with_isa(&input, isa.as_ref());
    let handler_binding = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LetStatement)
        .into_iter()
        .find(|binding| {
            beskid_queries::event_handler_lambda_for_local(db, *binding).expect("event handler binding query").is_some()
        })
        .expect("event handler let binding fact");
    let initializer = facts.let_initializer(handler_binding).expect("handler lambda initializer");
    assert_eq!(
        beskid_queries::node_kind(db, initializer).expect("handler initializer kind query"),
        Some(beskid_queries::IndexedNodeKind::LambdaExpression),
        "event wrapper branch selection requires the lambda initializer node"
    );
    assert!(
        facts.event_handler_local(handler_binding).is_some(),
        "event handler local plan must have an exact trampoline and capture allocation authority"
    );
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| beskid_queries::item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("capture-free event subscription lowering");
    let imports = artifact
        .extern_imports
        .iter()
        .chain(&artifact.trusted_extern_imports)
        .map(|import| import.symbol.as_str())
        .collect::<Vec<_>>();
    assert!(
        imports.contains(&"event_subscribe"),
        "event subscribe must use the manifest-authorized service: {imports:?}"
    );
    assert!(
        imports.contains(&"event_unsubscribe_first"),
        "event removal must use the manifest-authorized first-match service: {imports:?}"
    );
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("lowered Main");
    let clif = main.function.display().to_string();
    assert!(clif.contains("iconst.i64 4"), "capacity is sourced from event{{4}}:\n{clif}");
    assert!(clif.contains("event_subscribe"), "subscribe call appears in verified CLIF:\n{clif}");
    assert!(clif.contains("event_unsubscribe_first"), "unsubscribe call appears in verified CLIF:\n{clif}");
}

const EVENT_SOURCE: &str = r#"type User { event{4} Created(string payload) }
unit Main(i64 captured) {
    User u = User { };
    unit(string) boom = (string payload) => { captured; return; };
    u.Created += boom;
    u.Created -= boom;
    u.Created("payload");
    return;
}
"#;

#[test]
fn parsed_event_subscribe_unsubscribe_and_raise_import_the_canonical_abi_at_their_source_sites() {
    let (input, isa, root) = item_fixture_with_root(EVENT_SOURCE);
    let db = input.database();
    let assignments = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::AssignExpression);
    let calls = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression);
    let event_sites = assignments
        .iter()
        .copied()
        .filter_map(|key| {
            let site = format_ast_node_site(db, key);
            (site.contains("AssignExpression@5:") || site.contains("AssignExpression@6:")).then_some(site)
        })
        .collect::<Vec<_>>();
    let raise_site = calls
        .iter()
        .copied()
        .map(|key| format_ast_node_site(db, key))
        .find(|site| site.contains("CallExpression@7:"))
        .expect("the raise operation keeps its source span");

    assert_eq!(event_sites.len(), 2, "subscribe and unsubscribe sites: {event_sites:?}");
    assert!(event_sites.iter().any(|site| site.contains("AssignExpression@5:")), "{event_sites:?}");
    assert!(event_sites.iter().any(|site| site.contains("AssignExpression@6:")), "{event_sites:?}");
    assert!(raise_site.contains("CallExpression@7:"), "{raise_site}");

    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| beskid_queries::item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .unwrap_or_else(|error| panic!("event lowering at {event_sites:?} and {raise_site}: {error}"));
    let imports = artifact
        .extern_imports
        .iter()
        .chain(&artifact.trusted_extern_imports)
        .map(|import| import.symbol.as_str())
        .collect::<Vec<_>>();

    for symbol in [
        "event_subscribe",
        "event_unsubscribe_first",
        "event_len",
        "event_get_handler",
        "beskid_rt_v5_closure_environment_allocate",
        "beskid_rt_v5_closure_capture_store",
        "beskid_rt_v5_managed_object_allocate",
        "gc_register_root",
        "gc_unregister_root",
    ] {
        assert!(imports.contains(&symbol), "expected canonical event import {symbol}; imports: {imports:?}");
    }
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("lowered Main");
    let clif = main.function.display().to_string();
    assert!(clif.contains("call_indirect"), "raise dynamically invokes the wrapper's trampoline:\n{clif}");
    assert!(clif.contains("iconst.i64 4"), "event capacity comes from its declaration:\n{clif}");
}

#[test]
fn captured_local_event_handler_materializes_at_declaration_and_lowers_in_aot_entry() {
    let source = r#"type User { event{4} created(string payload) }
impl User { unit Emit(string payload) { this.created(payload); } }
i32 Main() {
    User u = User { };
    string marker = "retained";
    unit(string) boom = (string payload) => { marker; };
    u.created += boom;
    u.Emit("before unsubscribe");
    u.created -= boom;
    u.Emit("after unsubscribe");
    return 0;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new_with_isa(&input, isa.as_ref());
    let handler_binding = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LetStatement)
        .into_iter()
        .find(|binding| {
            beskid_queries::event_handler_lambda_for_local(db, *binding).expect("event handler binding query").is_some()
        })
        .expect("event handler let binding fact");
    let initializer = facts.let_initializer(handler_binding).expect("handler lambda initializer");
    assert_eq!(
        beskid_queries::node_kind(db, initializer).expect("handler initializer kind query"),
        Some(beskid_queries::IndexedNodeKind::LambdaExpression),
        "event wrapper branch selection requires the lambda initializer node"
    );
    assert!(
        facts.event_handler_local(handler_binding).is_some(),
        "event handler local plan must have an exact trampoline and capture allocation authority"
    );
    let raise = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .find(|call| beskid_queries::event_operation(db, *call).expect("event raise query").is_some())
        .expect("event raise with a parameter-backed payload");
    let raise_fact = beskid_queries::event_operation(db, raise)
        .expect("event raise operation query")
        .expect("event raise operation fact");
    assert_eq!(
        beskid_queries::value_abi_type(db, raise_fact.arguments[0]).expect("raise argument ABI query"),
        Some(beskid_queries::SemanticTypeId::STRING),
        "a captured event's dynamic raise payload retains the declared string ABI"
    );
    assert_eq!(
        facts.call_kind(raise),
        Some(beskid_isle::CallKind::EventRaise),
        "event raise fact must outrank ordinary dynamic-call selection"
    );
    assert_eq!(
        facts.event_operation(raise).map(|plan| plan.operation),
        Some(beskid_isle::EventOperation::Raise),
        "NodeFacts must preserve the checked event raise fact"
    );
    let main = find_function_definitions(db, root)
        .into_iter()
        .find(|key| beskid_queries::item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let emit = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .into_iter()
        .find(|key| beskid_queries::item_name(db, *key).ok().flatten().as_deref() == Some("Emit"))
        .expect("User.Emit method");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: emit, symbol: "User_Emit".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("capturing event handler should lower with a stable wrapper");
    assert!(artifact.event_handler_wrapper_required);
    let imports = artifact
        .extern_imports
        .iter()
        .chain(&artifact.trusted_extern_imports)
        .map(|import| import.symbol.as_str())
        .collect::<Vec<_>>();
    for symbol in [
        "event_subscribe",
        "event_unsubscribe_first",
        "event_len",
        "event_get_handler",
        "beskid_rt_v5_closure_environment_allocate",
        "beskid_rt_v5_managed_object_allocate",
        "gc_register_root",
        "gc_unregister_root",
    ] {
        assert!(imports.contains(&symbol), "missing event wrapper import `{symbol}` in {imports:?}");
    }
}

#[test]
fn event_operation_resolves_each_local_lambda_initializer_for_stable_handler_materialization() {
    let source = r#"type User { event{4} Created(string payload) }
unit Main(i64 captured) {
    User u = User { };
    unit(string) first = (string payload) => { captured; return; };
    unit(string) second = (string payload) => { captured; return; };
    u.Created += first;
    u.Created += second;
    u.Created -= first;
    return;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let operations = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::AssignExpression)
        .into_iter()
        .filter_map(|key| beskid_queries::event_operation(db, key).expect("event query").map(|fact| (key, fact)))
        .collect::<Vec<_>>();
    assert_eq!(operations.len(), 3);
    let first = operations[0].1.handler_lambda.expect("first local lambda initializer");
    let second = operations[1].1.handler_lambda.expect("second local lambda initializer");
    let removed = operations[2].1.handler_lambda.expect("unsubscribe resolves the same local initializer");
    assert_ne!(first, second, "distinct local closures with identical signatures need distinct identities");
    assert_eq!(first, removed, "subscribe and unsubscribe use the same binding identity");
    assert_eq!(
        beskid_queries::closure_environment(db, first)
            .expect("first closure environment")
            .expect("capturing handler")
            .captures
            .len(),
        1,
        "the stored wrapper must retain the lambda's capture environment"
    );
    assert_eq!(
        beskid_queries::closure_environment(db, second)
            .expect("second closure environment")
            .expect("capturing handler")
            .captures
            .len(),
        1
    );
    let event_handler_lambdas = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LetStatement)
        .into_iter()
        .filter_map(|binding| {
            beskid_queries::event_handler_lambda_for_local(db, binding)
                .expect("event handler binding query")
                .map(|handler| (binding, handler.lambda))
        })
        .collect::<Vec<_>>();
    assert_eq!(event_handler_lambdas.len(), 2, "only local bindings used by event operations are selected");
    assert!(event_handler_lambdas.iter().any(|(_, lambda)| *lambda == first));
    assert!(event_handler_lambdas.iter().any(|(_, lambda)| *lambda == second));
    let facts = beskid_codegen::isle_adapter::SyntaxNodeFacts::new_with_isa(&input, isa.as_ref());
    assert!(facts.lambda_entry(first).is_some(), "captured lambda must have a closure trampoline plan");
    for (binding, _) in event_handler_lambdas {
        assert!(facts.event_handler_local(binding).is_some(), "resolved event handler local must have a lowering plan");
    }
}
