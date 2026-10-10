//! Specialized generic free-function bodies: methods on locals typed by the function's own type
//! parameters, and type-qualified calls to functions of the declaring unit.

use super::super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

const BAG: &str = "type Bag<T> { T[] values, i64 count, Bag<T> Put(T next) { return Bag<T> { values: values, count: count + 1 }; } i64 Size() { return count; } } T[] NoValues<T>() { return []; } Bag<T> Empty<T>() { return Bag<T> { values: NoValues<T>(), count: 0 }; }";

/// Lower every method and function of one fixture and return the emitted function names.
fn lower_all(source: &str) -> Vec<String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let mut keys = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition);
    keys.extend(find_function_definitions(input.database(), root));
    let items = keys
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
        })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .unwrap_or_else(|error| panic!("specialized generic bodies lower: {error:?}"));
    beskid_codegen::validate_artifact(&artifact).expect("specialized generic artifact resolves its imports");
    artifact.functions.iter().map(|function| function.name.to_string()).collect()
}

#[test]
fn generic_function_calls_a_method_on_a_local_typed_by_its_own_parameter() {
    let names = lower_all(&format!(
        "{BAG} Bag<T> Single<T>(T value) {{ Bag<T> bag = Empty<T>(); return bag.Put(value); }} i64 Main() {{ Bag<i64> bag = Single<i64>(7_i64); Bag<string> text = Single<string>(\"s\"); return bag.Size() + text.Size(); }}"
    ));
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Single#generic_")).count(),
        2,
        "one Single specialization each for i64 and string: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Put#generic_")).count(),
        2,
        "the receiver's T is the enclosing binding, so Put specializes for i64 and string: {names:?}"
    );
}

#[test]
fn generic_function_reassigns_a_mutable_local_through_its_own_method() {
    let names = lower_all(&format!(
        "{BAG} Bag<T> Twice<T>(T value) {{ mut Bag<T> result = Empty<T>(); result = result.Put(value); result = result.Put(value); return result; }} i64 Main() {{ Bag<i64> bag = Twice<i64>(3_i64); return bag.Size(); }}"
    ));
    assert!(names.iter().any(|name| name.starts_with("Twice#generic_")), "{names:?}");
    assert!(names.iter().any(|name| name.starts_with("Put#generic_")), "{names:?}");
}

#[test]
fn generic_function_passes_a_generic_call_result_to_a_method_on_its_local() {
    let names = lower_all(&format!(
        "{BAG} T First<T>(T[] values) {{ return values[0]; }} Bag<T> Head<T>(T[] values) {{ Bag<T> bag = Empty<T>(); return bag.Put(First<T>(values)); }} i64 Main() {{ i64[] values = [5_i64, 6_i64]; Bag<i64> bag = Head<i64>(values); return bag.Size(); }}"
    ));
    assert!(names.iter().any(|name| name.starts_with("Head#generic_")), "{names:?}");
    assert!(names.iter().any(|name| name.starts_with("First#generic_")), "{names:?}");
}

#[test]
fn type_qualified_call_to_a_function_of_the_declaring_unit_specializes() {
    let names = lower_all(&format!(
        "{BAG} Bag<T> One<T>(T value) {{ Bag<T> bag = Bag.Empty<T>(); return bag.Put(value); }} Bag<T> Wrap<T>(T value) {{ return Bag.One<T>(value); }} i64 Main() {{ Bag<i64> bag = Wrap<i64>(4_i64); Bag<string> text = Wrap<string>(\"s\"); return bag.Size() + text.Size(); }}"
    ));
    assert_eq!(
        names.iter().filter(|name| name.starts_with("One#generic_")).count(),
        2,
        "`Bag.One<T>` inside a specialized body names this unit's `One`: {names:?}"
    );
    assert_eq!(names.iter().filter(|name| name.starts_with("Empty#generic_")).count(), 2, "{names:?}");
}
