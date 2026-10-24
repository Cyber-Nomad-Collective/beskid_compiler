//! Copy on assignment (v0.6.7): a place read of a `[CopyOnAssign]` value in a copy position
//! calls the type's `Copy` hook instance, which module emission collects without a call node.

use super::super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

const BAG: &str = "[CopyOnAssign] type Bag<T> { T[] values; i64 count; \
    Bag<T> Copy() { return Bag<T> { values: this.values, count: this.count }; } \
    unit Put(mut this, T next) { this.count = this.count + 1; return; } } \
    T[] NoValues<T>() { return []; } \
    Bag<T> Empty<T>() { return Bag<T> { values: NoValues<T>(), count: 0 }; } ";

/// Lower the fixture's functions (and every method, so a hook is lowered only when reached) and
/// return each emitted function's name with its CLIF text.
fn lower(source: &str) -> Vec<(String, String)> {
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
        .unwrap_or_else(|error| panic!("copy on assignment lowers: {error:?}"));
    beskid_codegen::validate_artifact(&artifact).expect("copy hook imports resolve");
    artifact.functions.iter().map(|function| (function.name.clone(), function.function.display().to_string())).collect()
}

fn calls_in(functions: &[(String, String)], name: &str) -> usize {
    let (_, text) = functions.iter().find(|(function, _)| function == name).unwrap_or_else(|| panic!("{name}"));
    text.lines().filter(|line| line.contains(" call ")).count()
}

#[test]
fn binding_assignment_argument_and_construction_reads_call_the_copy_hook() {
    let functions = lower(&format!(
        "{BAG} type Holder {{ Bag<i64> bag; }} unit Keep(Bag<i64> kept) {{ return; }} \
         i64 Copied(Bag<i64> a) {{ Bag<i64> b = a; mut Bag<i64> c = Empty<i64>(); c = a; Keep(a); Holder h = Holder {{ bag: a }}; return b.count + c.count + h.bag.count; }} \
         i64 Shared(Bag<i64> a) {{ a.Put(1); return a.count; }}"
    ));
    let names = functions.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Copy#generic_")).count(),
        1,
        "one Copy instance for Bag<i64>: {names:?}"
    );
    // `Empty<i64>()`, `Keep`, and four copies (binding, assignment, argument, construction).
    assert!(calls_in(&functions, "Copied") >= 6, "{:?}", functions.iter().find(|(name, _)| name == "Copied"));
}

#[test]
fn receivers_mut_parameters_and_owned_returns_do_not_copy() {
    let functions = lower(&format!(
        "{BAG} unit Grow(mut Bag<i64> b) {{ b.Put(9); return; }} Bag<i64> Own(Bag<i64> b) {{ return b; }} \
         i64 Main(Bag<i64> a) {{ a.Put(1); Grow(a); return a.count; }}"
    ));
    let names = functions.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    assert!(!names.iter().any(|name| name.starts_with("Copy#")), "nothing copies: {names:?}");
}

#[test]
fn a_type_parameter_read_copies_in_the_specialization_that_binds_it_to_a_copy_on_assign_type() {
    let functions = lower(&format!(
        "{BAG} T First<T>(T[] values) {{ return values[0]; }} \
         i64 Main(Bag<i64>[] bags, i64[] numbers) {{ Bag<i64> first = First<Bag<i64>>(bags); return first.count + First<i64>(numbers); }}"
    ));
    let names = functions.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    assert_eq!(names.iter().filter(|name| name.starts_with("First#generic_")).count(), 2, "{names:?}");
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Copy#generic_")).count(),
        1,
        "the returned element copies only in First<Bag<i64>>: {names:?}"
    );
}

#[test]
fn a_for_element_and_a_closure_capture_call_the_copy_hook() {
    let functions = lower(&format!(
        "{BAG} i64 Loop(Bag<i64>[] bags) {{ mut i64 total = 0; for bag in bags {{ total = total + bag.count; }} return total; }} \
         i64 Apply(() => i64 read) {{ return read(); }} \
         i64 Capture(Bag<i64> captured) {{ return Apply(() => captured.count); }}"
    ));
    let names = functions.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Copy#generic_")).count(),
        1,
        "the loop element and the capture both call the Bag<i64> Copy instance: {names:?}"
    );
    assert!(calls_in(&functions, "Loop") >= 1, "{:?}", functions.iter().find(|(name, _)| name == "Loop"));
}

#[test]
fn a_match_pattern_binding_calls_the_copy_hook() {
    let functions = lower(&format!(
        "{BAG} enum Slot {{ Full(Bag<i64> bag), Empty }} \
         i64 Held(Slot slot) {{ return match slot {{ Slot::Full(held) => held.count, Slot::Empty => 0, }}; }}"
    ));
    let names = functions.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Copy#generic_")).count(),
        1,
        "the pattern binding calls the Bag<i64> Copy instance: {names:?}"
    );
    assert!(calls_in(&functions, "Held") >= 1, "{:?}", functions.iter().find(|(name, _)| name == "Held"));
}
