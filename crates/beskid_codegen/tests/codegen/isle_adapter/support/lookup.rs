use super::prelude::{AstNodeKey, BeskidDatabase, Db, call_lowering, child_nodes, literal_fact, node_kind};

pub(in super::super) fn find_function_definitions(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Vec<AstNodeKey> {
    let mut found = Vec::new();
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::FunctionDefinition) {
        found.push(key);
    }
    if let Some(children) = child_nodes(db, key).ok().flatten() {
        for child in children.iter().copied() {
            found.extend(find_function_definitions(db, child));
        }
    }
    found
}

pub(in super::super) fn find_definition_of_kind(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    expected: beskid_queries::IndexedNodeKind,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(expected) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_definition_of_kind(db, child, expected))
}

pub(in super::super) fn find_nodes_of_kind(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    expected: beskid_queries::IndexedNodeKind,
) -> Vec<AstNodeKey> {
    let mut nodes = Vec::new();
    if node_kind(db, key).ok().flatten() == Some(expected) {
        nodes.push(key);
    }
    if let Some(children) = child_nodes(db, key).ok().flatten() {
        for child in children.iter().copied() {
            nodes.extend(find_nodes_of_kind(db, child, expected));
        }
    }
    nodes
}

pub(in super::super) fn find_call_expression(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::CallExpression) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_call_expression(db, child))
}

pub(in super::super) fn find_corelib_service_call(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    expected_name: &str,
) -> Option<AstNodeKey> {
    if matches!(
        call_lowering(db, key).ok().flatten(),
        Some(beskid_queries::CallLowering::CorelibService(service)) if service.name == expected_name
    ) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_corelib_service_call(db, child, expected_name))
}

pub(in super::super) fn find_function_definition(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten().is_some_and(|kind| kind == beskid_queries::IndexedNodeKind::FunctionDefinition)
    {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_function_definition(db, child))
}

pub(in super::super) fn find_test_definition(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten().is_some_and(|kind| kind == beskid_queries::IndexedNodeKind::TestDefinition) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_test_definition(db, child))
}

pub(in super::super) fn find_integer_literal(db: &BeskidDatabase, key: AstNodeKey) -> Option<AstNodeKey> {
    if literal_fact(db, key)
        .ok()
        .flatten()
        .is_some_and(|fact| matches!(fact, beskid_queries::LiteralFact::Integer(value) if value.as_ref() == "42"))
    {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_integer_literal(db, child))
}

pub(in super::super) fn find_node(
    db: &dyn Db,
    key: AstNodeKey,
    expected: beskid_queries::IndexedNodeKind,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(expected) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().find_map(|child| find_node(db, *child, expected))
}
