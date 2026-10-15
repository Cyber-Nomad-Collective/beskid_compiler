//! Named and constructor enum elements need traced descriptors from canonical managed facts.
#[path = "isle_adapter/support.rs"]
mod support;
use beskid_queries::{IndexedNodeKind, SemanticTypeId};
use support::{find_function_definition, find_nodes_of_kind, item_fixture_with_root};

#[test]
fn named_enum_array_has_one_managed_pointer_per_element() {
    let (input, isa, root) = item_fixture_with_root(
        "enum DataValue { Unit, Optional(bool present, DataValue[] payload), } unit Main() { DataValue value = DataValue::Unit(); DataValue[] values = [value]; }",
    );
    let item = find_function_definition(input.database(), root).unwrap();
    let arrays = find_nodes_of_kind(input.database(), item, IndexedNodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 1);
    let plan = input.array_static_plan(arrays[0]).expect("named enum must retain canonical managed element authority");
    assert_eq!(plan.element_type, SemanticTypeId::POINTER);
    assert_eq!(plan.length, 1);
    assert_eq!(plan.stride, u64::from(isa.pointer_type().bytes()));
    assert_eq!(plan.alignment, plan.stride);
    assert_eq!(plan.pointer_map_offsets.as_ref(), &[0]);
}

#[test]
fn recursive_enum_constructor_arrays_trace_outer_and_nested_payloads() {
    let (input, isa, root) = item_fixture_with_root(
        "enum DataValue { Unit, Optional(bool present, DataValue[] payload), } unit Main() { DataValue[] values = [DataValue::Optional(true, [DataValue::Unit()])]; }",
    );
    let item = find_function_definition(input.database(), root).unwrap();
    let arrays = find_nodes_of_kind(input.database(), item, IndexedNodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 2);
    for array in arrays {
        let plan = input.array_static_plan(array).unwrap_or_else(|| {
            let elements = beskid_queries::child_nodes(input.database(), array).unwrap().unwrap();
            let facts = elements
                .iter()
                .map(|element| {
                    (
                        *element,
                        beskid_queries::node_type(input.database(), *element),
                        beskid_queries::abi_type(input.database(), *element),
                        beskid_queries::managed_reference_kind(input.database(), *element),
                    )
                })
                .collect::<Vec<_>>();
            panic!("every recursive enum array is traced: {facts:?}")
        });
        assert_eq!(plan.element_type, SemanticTypeId::POINTER);
        assert_eq!(plan.stride, u64::from(isa.pointer_type().bytes()));
        assert_eq!(plan.pointer_map_offsets.as_ref(), &[0]);
    }
}

#[test]
fn scalar_arrays_remain_untraced_and_mixed_enum_scalar_arrays_fail_closed() {
    let (input, _isa, root) = item_fixture_with_root(
        "enum DataValue { Unit, } unit Main() { DataValue value = DataValue::Unit(); i64[] words = [1_i64, 2_i64]; [value, 1_i64]; }",
    );
    let item = find_function_definition(input.database(), root).unwrap();
    let arrays = find_nodes_of_kind(input.database(), item, IndexedNodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 2);
    let scalar = input.array_static_plan(arrays[0]).expect("scalar control retains exact metadata");
    assert_eq!(scalar.element_type, SemanticTypeId::I64);
    assert_eq!(scalar.stride, 8);
    assert!(scalar.pointer_map_offsets.is_empty());
    assert!(input.array_static_plan(arrays[1]).is_none(), "managed and scalar ABI cannot be conflated");
}

#[test]
fn native_pointer_array_never_acquires_a_gc_pointer_map_by_width() {
    let (input, _isa, root) = item_fixture_with_root("unit Main(pointer native) { pointer[] values = [native]; }");
    let item = find_function_definition(input.database(), root).unwrap();
    let array = find_nodes_of_kind(input.database(), item, IndexedNodeKind::ArrayLiteralExpression)[0];
    if let Some(plan) = input.array_static_plan(array) {
        assert!(plan.pointer_map_offsets.is_empty(), "opaque native pointer must remain untraced");
    }
}

#[test]
fn undefined_nominal_and_value_elements_do_not_authorize_descriptors() {
    for source in ["unit Main() { [missing]; }", "unit Main() { [Missing::Unit()]; }"] {
        let (input, _isa, root) = item_fixture_with_root(source);
        let item = find_function_definition(input.database(), root).unwrap();
        let array = find_nodes_of_kind(input.database(), item, IndexedNodeKind::ArrayLiteralExpression)[0];
        assert!(input.array_static_plan(array).is_none(), "unproved element remains unavailable: {source}");
    }
}
