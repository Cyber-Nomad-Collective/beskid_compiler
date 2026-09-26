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
    assert_eq!(
        facts.assignment_kind(operations[0].operation_node),
        Some(beskid_isle::AssignmentKind::EventSubscribe)
    );
    assert_eq!(
        facts.assignment_kind(operations[1].operation_node),
        Some(beskid_isle::AssignmentKind::EventUnsubscribeFirst)
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
    let stale = beskid_queries::AstNodeKey { generation: beskid_analysis::syntax::SyntaxGenerationId(u64::MAX), ..operation };
    assert!(facts.event_operation(stale).is_none(), "stale generations do not select the event rule");

    let (unbounded_input, _isa, unbounded_root) = item_fixture_with_root(
        "type User { event Changed() } unit Main(User u) { unit() handler = () => { return; }; u.Changed += handler; return; }",
    );
    let unbounded_db = unbounded_input.database();
    let unbounded_operation = find_nodes_of_kind(
        unbounded_db,
        unbounded_root,
        beskid_queries::IndexedNodeKind::AssignExpression,
    )
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

const EVENT_SOURCE: &str = r#"type User { event{4} Created(string payload) }
unit Main() {
    User u = User { };
    unit(string) boom = (string payload) => { return; };
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

    for symbol in ["event_subscribe", "event_unsubscribe_first", "event_len", "event_get_handler"] {
        assert!(imports.contains(&symbol), "expected canonical event import {symbol}; imports: {imports:?}");
    }
}
