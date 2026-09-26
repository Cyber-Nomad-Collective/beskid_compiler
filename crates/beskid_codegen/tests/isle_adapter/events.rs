use super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, format_ast_node_site, item_fixture_with_root,
    lower_syntax_program,
};

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
