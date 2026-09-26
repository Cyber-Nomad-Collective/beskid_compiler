use super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, format_ast_node_site, item_fixture_with_root,
    lower_syntax_program,
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
