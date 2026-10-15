//! Equal user declarations cannot issue reserved recovery allocation authority.
#[path = "isle_adapter/support.rs"]
mod support;
use beskid_queries::IndexedNodeKind;
use support::{find_function_definition, find_nodes_of_kind, item_fixture_with_root};

#[test]
fn user_failure_constructor_cannot_issue_reserved_failure_plan() {
    let (input, _, root) = item_fixture_with_root(
        "enum SerializationError { AllocationFailure, } enum Result<T,E> { Ok(T value), Error(E error), } Result<i64,SerializationError> Main() { return Result::Error(SerializationError::AllocationFailure); }",
    );
    let item = find_function_definition(input.database(), root).unwrap();
    assert!(
        input.checked_failure_destination_plan(item, None).is_none(),
        "user return-type names cannot issue specialized canonical failure destination"
    );
    let constructors = find_nodes_of_kind(input.database(), item, IndexedNodeKind::EnumConstructorExpression);
    assert_eq!(constructors.len(), 2);
    for call in constructors {
        assert!(
            input.reserved_failure_static_plan(call, None).is_none(),
            "same name and layout cannot authorize reserved failure"
        );
    }
}
