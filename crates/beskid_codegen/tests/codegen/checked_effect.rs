#[path = "isle_adapter/support.rs"]
mod support;
use support::{find_function_definition, item_fixture_with_root};

#[test]
fn exact_recursive_source_closure_is_finite_no_yield() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Main(i32 value) { if value == 0 { return 0; } return Main(value - 1); }");
    let entry = find_function_definition(input.database(), root).unwrap();
    let proof = input.checked_effect_closure(isa.as_ref(), entry, None).unwrap();
    assert_eq!(proof.members().count(), 1);
    assert!(proof.visited_nodes() > 1);
    assert_eq!(proof.item(proof.entry()).unwrap().key(), entry);
}

#[test]
fn reachable_direct_callee_body_is_part_of_effect_proof() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Main() { return Child(41); } i32 Child(i32 value) { return value + 1; }");
    let entry = find_function_definition(input.database(), root).unwrap();
    let proof = input.checked_effect_closure(isa.as_ref(), entry, None).unwrap();
    assert_eq!(proof.members().count(), 2);
}

#[test]
fn lambda_callback_has_no_implicit_recoverable_effect() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Main() { let add = (i32 value) => value + 1; return add(41); }");
    let entry = find_function_definition(input.database(), root).unwrap();
    let rejection = input.checked_effect_closure(isa.as_ref(), entry, None).unwrap_err();
    assert!(matches!(
        rejection.reason,
        "unproved yield, callback or cleanup effect" | "open or unresolved call effect"
    ));
}

#[test]
fn rollback_numeric_body_has_stricter_nonallocating_proof() {
    let (input, isa, root) = item_fixture_with_root("i32 Main(i32 value) { return value + 1; }");
    let entry = find_function_definition(input.database(), root).unwrap();
    assert!(input.nonallocating_publication(isa.as_ref(), entry, None).is_ok());
}

#[test]
fn no_yield_does_not_authorize_allocating_publication() {
    let (input, isa, root) = item_fixture_with_root("string Main() { return \"allocated\"; }");
    let entry = find_function_definition(input.database(), root).unwrap();
    assert!(input.checked_effect_closure(isa.as_ref(), entry, None).is_ok());
    assert_eq!(
        input.nonallocating_publication(isa.as_ref(), entry, None).unwrap_err().reason,
        "publication constructs a managed value"
    );
}

#[test]
fn user_encode_name_does_not_issue_publication_methods() {
    use std::sync::Arc;
    let (input, isa, root) = item_fixture_with_root("i32 Encode() { return 1; }");
    let entry = find_function_definition(input.database(), root).unwrap();
    let instance = beskid_queries::GenericSpecializationInstance {
        declaration: entry,
        declaration_identity: Arc::from("User.Encode"),
        signature: beskid_queries::ItemSignature {
            parameters: Arc::from([]),
            result: beskid_queries::SemanticTypeId::I32,
        },
        substitutions: Arc::from([]),
        contract_witnesses: Arc::from([]),
    };
    assert!(input.serialization_publication_plan(isa.as_ref(), instance).is_err());
}

#[test]
fn bulk_argument_packing_is_allocation_even_for_scalar_callee() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 Main() { return Child(1, 2); } i32 Child(bulk i32[] values) { return values[0] + values[1]; }",
    );
    let entry = find_function_definition(input.database(), root).unwrap();
    assert!(input.checked_effect_closure(isa.as_ref(), entry, None).is_ok());
    assert_eq!(
        input.nonallocating_publication(isa.as_ref(), entry, None).unwrap_err().reason,
        "publication allocates an array"
    );
}
